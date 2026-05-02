//! `domain::reminders` — reminder scheduling and notification engine.
//!
//! Port of `@helm/reminders` TypeScript module. Computes next
//! occurrences (with RRULE support via the calendar engine),
//! builds notification payloads, and detects overdue reminders.

pub mod engine;
pub mod types;

pub use engine::{
    build_notification_payload, compute_next_occurrence, due_today, find_overdue, is_snoozed,
    widget_contributions,
};
pub use types::{NotificationPayload, OverdueInfo, Reminder, ReminderPriority, ReminderStatus};
