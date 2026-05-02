//! Pure data types for the focus planner engine.
//!
//! Port of `@helm/focus-planner` types. Daily planning context,
//! planning methods (MIT/3-things/time-blocking), check-ins, brain
//! dump, and scoring.

use serde::{Deserialize, Serialize};

// ── Planning Method ──────────────────────────────────────────────

/// Which planning methodology governs the daily plan.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PlanningMethod {
    /// Most Important Task — pick one key task.
    Mit,
    /// Pick 3 priorities.
    ThreeThings,
    /// Assign tasks to time blocks.
    TimeBlocking,
    /// User-defined method.
    Custom { name: String },
}

// ── Daily Plan ───────────────────────────────────────────────────

/// A single day's focus plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyPlan {
    /// ISO date (`YYYY-MM-DD`).
    pub date: String,
    pub method: PlanningMethod,
    pub items: Vec<PlanItem>,
    pub check_ins: Vec<CheckIn>,
    pub brain_dump: Vec<String>,
    pub reflection: Option<String>,
}

// ── Plan Item ────────────────────────────────────────────────────

/// An individual item within a daily plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub id: String,
    pub title: String,
    /// Optional link to a source object (task, event, etc.).
    pub object_id: Option<String>,
    /// Type of the linked object.
    pub object_type: Option<String>,
    /// 1 (highest) to 5 (lowest).
    pub priority: u8,
    pub completed: bool,
    pub time_block: Option<TimeBlock>,
}

// ── Time Block ───────────────────────────────────────────────────

/// A scheduled window within a day.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBlock {
    pub start_hour: u8,
    pub start_minute: u8,
    pub duration_minutes: u32,
}

// ── Check-In ─────────────────────────────────────────────────────

/// A point-in-time energy/focus/mood self-report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckIn {
    /// ISO datetime.
    pub time: String,
    /// 1 (low) to 5 (high).
    pub energy_level: u8,
    /// 1 (low) to 5 (high).
    pub focus_level: u8,
    /// 1 (low) to 5 (high).
    pub mood: u8,
    pub notes: Option<String>,
}

// ── Context Source ───────────────────────────────────────────────

/// Where a daily-context item originates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSource {
    Task,
    Event,
    Habit,
    Reminder,
}

// ── Daily Context Item ───────────────────────────────────────────

/// One entry in the merged daily context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyContextItem {
    pub title: String,
    pub source: ContextSource,
    /// 1 (highest) to 5 (lowest).
    pub priority: u8,
    /// Optional ISO datetime or `HH:MM` for scheduling.
    pub time: Option<String>,
}

// ── Daily Context ────────────────────────────────────────────────

/// Merged view of everything relevant to a given day.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyContext {
    /// ISO date (`YYYY-MM-DD`).
    pub date: String,
    pub items: Vec<DailyContextItem>,
    pub task_count: u32,
    pub event_count: u32,
    pub habit_count: u32,
    pub reminder_count: u32,
}

// ── Plan Score ───────────────────────────────────────────────────

/// Summary metrics for a completed daily plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanScore {
    /// Fraction of items completed (0.0..=1.0).
    pub completion_rate: f64,
    /// Mean energy level from check-ins (1.0..=5.0), or 0.0 if none.
    pub avg_energy: f64,
    /// Mean focus level from check-ins (1.0..=5.0), or 0.0 if none.
    pub avg_focus: f64,
    pub items_planned: u32,
    pub items_completed: u32,
}
