//! Pure data types for the reminders engine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReminderStatus {
    Active,
    Snoozed,
    Completed,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReminderPriority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub due_date: String,
    pub recurrence: Option<crate::domain::calendar::types::RecurrenceRule>,
    pub status: ReminderStatus,
    pub object_id: Option<String>,
    pub object_type: Option<String>,
    pub priority: ReminderPriority,
    pub snoozed_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    pub title: String,
    pub body: String,
    pub priority: ReminderPriority,
    pub object_id: Option<String>,
    pub object_type: Option<String>,
    pub due_date: String,
    pub is_overdue: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverdueInfo {
    pub reminder_id: String,
    pub days_overdue: i64,
    pub priority: ReminderPriority,
}
