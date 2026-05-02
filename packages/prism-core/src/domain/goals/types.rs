//! Pure data types for the goals engine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GoalStatus {
    NotStarted,
    InProgress,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ProgressMode {
    Manual { progress: f64 },
    Milestones,
    Children,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub title: String,
    pub description: String,
    pub parent_id: Option<String>,
    pub status: GoalStatus,
    pub target_date: Option<String>,
    pub created_at: String,
    pub progress_mode: ProgressMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: String,
    pub goal_id: String,
    pub title: String,
    pub completed: bool,
    pub target_date: Option<String>,
    pub completed_at: Option<String>,
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalProgress {
    pub goal_id: String,
    pub progress: f64,
    pub milestones_completed: u32,
    pub milestones_total: u32,
    pub children_completed: u32,
    pub children_total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeadlineAlert {
    Overdue,
    DueSoon,
    OnTrack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalAlert {
    pub goal_id: String,
    pub alert: DeadlineAlert,
    pub days_until_due: i64,
}
