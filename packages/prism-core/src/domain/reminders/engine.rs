//! Reminders engine — next occurrence, notification payloads, overdue.
//!
//! Port of `@helm/reminders` TypeScript module. Depends on the
//! calendar engine for RRULE expansion.

use chrono::NaiveDate;

use super::types::*;
use crate::domain::calendar::engine::parse_date_str;
use crate::domain::calendar::types::{CalendarEvent, DateRange};

pub fn compute_next_occurrence(reminder: &Reminder, after: &NaiveDate) -> Option<NaiveDate> {
    let due = parse_date_str(&reminder.due_date)?;

    if let Some(rule) = &reminder.recurrence {
        let event = CalendarEvent {
            object_id: reminder.id.clone(),
            title: reminder.title.clone(),
            start: due,
            end: due,
            all_day: true,
            color: None,
            event_type: "reminder".into(),
        };
        let range = DateRange {
            from: *after,
            to: after
                .checked_add_days(chrono::Days::new(365 * 5))
                .unwrap_or(*after),
        };
        let occurrences = crate::domain::calendar::engine::expand_recurring(&event, rule, &range);
        occurrences
            .into_iter()
            .map(|o| o.instance_date)
            .find(|d| d >= after)
    } else if due >= *after {
        Some(due)
    } else {
        None
    }
}

pub fn build_notification_payload(reminder: &Reminder, today: &NaiveDate) -> NotificationPayload {
    let due = parse_date_str(&reminder.due_date);
    let is_overdue = due.map(|d| d < *today).unwrap_or(false);

    let body = if is_overdue {
        let days = due.map(|d| (*today - d).num_days()).unwrap_or(0);
        format!(
            "{} — overdue by {} day{}",
            reminder.description.as_deref().unwrap_or(""),
            days,
            if days == 1 { "" } else { "s" }
        )
    } else {
        reminder.description.clone().unwrap_or_default()
    };

    NotificationPayload {
        title: reminder.title.clone(),
        body,
        priority: reminder.priority,
        object_id: reminder.object_id.clone(),
        object_type: reminder.object_type.clone(),
        due_date: reminder.due_date.clone(),
        is_overdue,
    }
}

pub fn find_overdue(reminders: &[Reminder], today: &NaiveDate) -> Vec<OverdueInfo> {
    reminders
        .iter()
        .filter(|r| matches!(r.status, ReminderStatus::Active))
        .filter_map(|r| {
            let due = parse_date_str(&r.due_date)?;
            if due < *today {
                Some(OverdueInfo {
                    reminder_id: r.id.clone(),
                    days_overdue: (*today - due).num_days(),
                    priority: r.priority,
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn is_snoozed(reminder: &Reminder, today: &NaiveDate) -> bool {
    if reminder.status != ReminderStatus::Snoozed {
        return false;
    }
    reminder
        .snoozed_until
        .as_deref()
        .and_then(parse_date_str)
        .map(|until| *today < until)
        .unwrap_or(false)
}

pub fn due_today<'a>(reminders: &'a [Reminder], today: &NaiveDate) -> Vec<&'a Reminder> {
    reminders
        .iter()
        .filter(|r| matches!(r.status, ReminderStatus::Active))
        .filter(|r| {
            parse_date_str(&r.due_date)
                .map(|d| d == *today)
                .unwrap_or(false)
        })
        .collect()
}

// ── Widget contributions ─────────────────────────────────────────

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        DataQuery, FieldSpec, LayoutDirection, QuerySort, SignalSpec, TemplateNode, ToolbarAction,
        WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "reminder-list".into(),
            label: "Reminders".into(),
            description: "Upcoming reminders list".into(),
            icon: Some("bell".into()),
            category: WidgetCategory::Display,
            signals: vec![
                SignalSpec::new("reminder-selected", "A reminder was selected")
                    .with_payload(vec![FieldSpec::text("reminder_id", "Reminder ID")]),
                SignalSpec::new("reminder-completed", "A reminder was completed")
                    .with_payload(vec![FieldSpec::text("reminder_id", "Reminder ID")]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("add-reminder", "Add", "plus"),
                ToolbarAction::signal("refresh", "Refresh", "refresh"),
            ],
            default_size: WidgetSize::new(2, 2),
            data_query: Some(DataQuery {
                object_type: Some("reminder".into()),
                sort: vec![QuerySort {
                    field: "due_date".into(),
                    descending: false,
                }],
                ..Default::default()
            }),
            data_key: Some("reminders".into()),
            data_fields: vec![
                FieldSpec::text("title", "Title"),
                FieldSpec::text("due_date", "Due Date"),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Reminders", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "reminders".into(),
                            item_template: Box::new(TemplateNode::DataBinding {
                                field: "title".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            }),
                            empty_label: Some("No reminders".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "overdue-reminders".into(),
            label: "Overdue".into(),
            description: "Overdue reminders with urgency indicators".into(),
            icon: Some("alert-circle".into()),
            category: WidgetCategory::Display,
            signals: vec![
                SignalSpec::new("reminder-selected", "A reminder was selected")
                    .with_payload(vec![FieldSpec::text("reminder_id", "Reminder ID")]),
            ],
            toolbar_actions: vec![ToolbarAction::signal("dismiss-all", "Dismiss All", "x")],
            default_size: WidgetSize::new(2, 1),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![TemplateNode::Repeater {
                        source: "overdue".into(),
                        item_template: Box::new(TemplateNode::DataBinding {
                            field: "title".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        }),
                        empty_label: Some("All caught up".into()),
                    }],
                },
            },
            ..Default::default()
        },
    ]
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::calendar::types::{Frequency, RecurrenceRule};

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn make_reminder(id: &str, title: &str, due: &str) -> Reminder {
        Reminder {
            id: id.into(),
            title: title.into(),
            description: Some(format!("Desc for {title}")),
            due_date: due.into(),
            recurrence: None,
            status: ReminderStatus::Active,
            object_id: None,
            object_type: None,
            priority: ReminderPriority::Normal,
            snoozed_until: None,
        }
    }

    #[test]
    fn next_occurrence_one_time_future() {
        let r = make_reminder("r1", "Pay bill", "2026-06-01");
        let next = compute_next_occurrence(&r, &d("2026-05-01"));
        assert_eq!(next, Some(d("2026-06-01")));
    }

    #[test]
    fn next_occurrence_one_time_past() {
        let r = make_reminder("r1", "Past", "2026-04-01");
        let next = compute_next_occurrence(&r, &d("2026-05-01"));
        assert!(next.is_none());
    }

    #[test]
    fn next_occurrence_recurring() {
        let mut r = make_reminder("r1", "Weekly", "2026-05-01");
        r.recurrence = Some(RecurrenceRule {
            frequency: Frequency::Weekly,
            interval: 1,
            count: Some(10),
            ..Default::default()
        });
        let next = compute_next_occurrence(&r, &d("2026-05-10"));
        assert_eq!(next, Some(d("2026-05-15")));
    }

    #[test]
    fn next_occurrence_recurring_same_day() {
        let mut r = make_reminder("r1", "Daily", "2026-05-01");
        r.recurrence = Some(RecurrenceRule {
            frequency: Frequency::Daily,
            interval: 1,
            count: Some(30),
            ..Default::default()
        });
        let next = compute_next_occurrence(&r, &d("2026-05-15"));
        assert_eq!(next, Some(d("2026-05-15")));
    }

    #[test]
    fn notification_payload_not_overdue() {
        let r = make_reminder("r1", "Meeting", "2026-06-01");
        let payload = build_notification_payload(&r, &d("2026-05-01"));
        assert_eq!(payload.title, "Meeting");
        assert!(!payload.is_overdue);
        assert_eq!(payload.body, "Desc for Meeting");
    }

    #[test]
    fn notification_payload_overdue() {
        let r = make_reminder("r1", "Overdue task", "2026-04-29");
        let payload = build_notification_payload(&r, &d("2026-05-01"));
        assert!(payload.is_overdue);
        assert!(payload.body.contains("overdue by 2 days"));
    }

    #[test]
    fn find_overdue_mixed() {
        let reminders = vec![
            make_reminder("r1", "Past", "2026-04-01"),
            make_reminder("r2", "Future", "2026-06-01"),
            make_reminder("r3", "Also past", "2026-04-15"),
        ];
        let overdue = find_overdue(&reminders, &d("2026-05-01"));
        assert_eq!(overdue.len(), 2);
        assert_eq!(overdue[0].reminder_id, "r1");
        assert_eq!(overdue[0].days_overdue, 30);
        assert_eq!(overdue[1].reminder_id, "r3");
    }

    #[test]
    fn find_overdue_excludes_completed() {
        let mut r = make_reminder("r1", "Done", "2026-04-01");
        r.status = ReminderStatus::Completed;
        let overdue = find_overdue(&[r], &d("2026-05-01"));
        assert!(overdue.is_empty());
    }

    #[test]
    fn is_snoozed_active() {
        let mut r = make_reminder("r1", "Test", "2026-05-01");
        r.status = ReminderStatus::Snoozed;
        r.snoozed_until = Some("2026-05-10".into());
        assert!(is_snoozed(&r, &d("2026-05-05")));
        assert!(!is_snoozed(&r, &d("2026-05-15")));
    }

    #[test]
    fn is_snoozed_not_snoozed_status() {
        let r = make_reminder("r1", "Test", "2026-05-01");
        assert!(!is_snoozed(&r, &d("2026-05-05")));
    }

    #[test]
    fn due_today_filters() {
        let reminders = vec![
            make_reminder("r1", "Today", "2026-05-01"),
            make_reminder("r2", "Tomorrow", "2026-05-02"),
            make_reminder("r3", "Also today", "2026-05-01"),
        ];
        let today = due_today(&reminders, &d("2026-05-01"));
        assert_eq!(today.len(), 2);
        assert_eq!(today[0].id, "r1");
        assert_eq!(today[1].id, "r3");
    }

    #[test]
    fn due_today_empty() {
        let reminders = vec![make_reminder("r1", "Tomorrow", "2026-05-02")];
        let today = due_today(&reminders, &d("2026-05-01"));
        assert!(today.is_empty());
    }

    #[test]
    fn widget_contributions_count() {
        let widgets = widget_contributions();
        assert_eq!(widgets.len(), 2);
        assert_eq!(widgets[0].id, "reminder-list");
        assert_eq!(widgets[1].id, "overdue-reminders");
    }
}
