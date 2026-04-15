//! Pure-function formatter for [`ActivityEvent`]s.
//!
//! Port of `interaction/activity/activity-formatter.ts`. No DOM, no
//! templating — just string building. Locale-dependent date rendering
//! from the TS version is intentionally simplified to ISO-ish output
//! since Rust's `chrono` has no cross-platform locale facility.

use chrono::{DateTime, Datelike, TimeZone, Utc};
use serde_json::Value as JsonValue;

use super::log::{ActivityDescription, ActivityEvent, ActivityGroup, ActivityVerb, FieldChange};

const MAX_TEXT_LENGTH: usize = 60;

#[derive(Debug, Clone, Default)]
pub struct FormatActivityOptions<'a> {
    pub actor_name: Option<&'a str>,
    pub object_name: Option<&'a str>,
}

pub fn format_field_name(field: &str) -> String {
    let bare = field.strip_prefix("data.").unwrap_or(field);
    match bare {
        "parentId" => return "parent".to_string(),
        "endDate" => return "end date".to_string(),
        "createdAt" => return "created".to_string(),
        "deletedAt" => return "deleted".to_string(),
        "updatedAt" => return "updated".to_string(),
        _ => {}
    }

    let mut out = String::with_capacity(bare.len() + 4);
    let mut prev_lower = false;
    for c in bare.chars() {
        if c == '_' || c == '-' {
            out.push(' ');
            prev_lower = false;
            continue;
        }
        if c.is_ascii_uppercase() && prev_lower {
            out.push(' ');
        }
        out.extend(c.to_lowercase());
        prev_lower = c.is_ascii_lowercase();
    }
    out.trim().to_string()
}

pub fn format_field_value(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "(none)".to_string(),
        JsonValue::Bool(b) => {
            if *b {
                "yes".into()
            } else {
                "no".into()
            }
        }
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Array(items) => {
            if items.is_empty() {
                return "(empty)".into();
            }
            let shown: Vec<String> = items.iter().take(3).map(format_field_value).collect();
            let rest = items.len().saturating_sub(3);
            if rest > 0 {
                format!("{} and {} more", shown.join(", "), rest)
            } else {
                shown.join(", ")
            }
        }
        JsonValue::String(s) => format_string_value(s),
        JsonValue::Object(map) => {
            if map.is_empty() {
                return "(empty)".into();
            }
            let s = serde_json::to_string(value).unwrap_or_else(|_| "(object)".into());
            truncate(&s)
        }
    }
}

fn format_string_value(s: &str) -> String {
    if s.is_empty() {
        return "(empty)".into();
    }
    if let Some(dt) = parse_iso_datetime(s) {
        return dt.format("%b %-d, %Y %-H:%M").to_string();
    }
    if let Some(date) = parse_iso_date(s) {
        return date.format("%b %-d, %Y").to_string();
    }
    truncate(s)
}

fn parse_iso_datetime(s: &str) -> Option<DateTime<Utc>> {
    if s.len() < 16 {
        return None;
    }
    let bytes = s.as_bytes();
    let has_shape = bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':');
    if !has_shape {
        return None;
    }
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn parse_iso_date(s: &str) -> Option<chrono::NaiveDate> {
    if s.len() != 10 {
        return None;
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn truncate(s: &str) -> String {
    if s.chars().count() > MAX_TEXT_LENGTH {
        let trimmed: String = s.chars().take(MAX_TEXT_LENGTH).collect();
        format!("{trimmed}\u{2026}")
    } else {
        s.to_string()
    }
}

fn bold(s: &str) -> String {
    format!("<b>{s}</b>")
}

pub fn format_activity(
    event: &ActivityEvent,
    opts: FormatActivityOptions<'_>,
) -> ActivityDescription {
    let actor: String = opts
        .actor_name
        .map(str::to_string)
        .or_else(|| event.actor_name.clone())
        .unwrap_or_else(|| "Someone".to_string());
    let object = opts.object_name;

    let (text, html) = match event.verb {
        ActivityVerb::Created => {
            if let Some(obj) = object {
                (
                    format!("{actor} created \"{obj}\""),
                    format!("{} created {}", bold(&actor), bold(obj)),
                )
            } else {
                (
                    format!("{actor} created this"),
                    format!("{} created this", bold(&actor)),
                )
            }
        }
        ActivityVerb::Deleted => (
            format!("{actor} deleted this"),
            format!("{} deleted this", bold(&actor)),
        ),
        ActivityVerb::Restored => (
            format!("{actor} restored this"),
            format!("{} restored this", bold(&actor)),
        ),
        ActivityVerb::Renamed => {
            if let Some(nc) = event.changes.iter().find(|c| c.field == "name") {
                let before = format_field_value(&nc.before);
                let after = format_field_value(&nc.after);
                (
                    format!("{actor} renamed from \"{before}\" to \"{after}\""),
                    format!(
                        "{} renamed from {} to {}",
                        bold(&actor),
                        bold(&before),
                        bold(&after)
                    ),
                )
            } else {
                (
                    format!("{actor} renamed this"),
                    format!("{} renamed this", bold(&actor)),
                )
            }
        }
        ActivityVerb::StatusChanged => {
            let from = event
                .from_status
                .as_ref()
                .map(|s| format_field_value(&JsonValue::String(s.clone())))
                .unwrap_or_else(|| "(none)".into());
            let to = event
                .to_status
                .as_ref()
                .map(|s| format_field_value(&JsonValue::String(s.clone())))
                .unwrap_or_else(|| "(none)".into());
            (
                format!("{actor} changed status from \"{from}\" to \"{to}\""),
                format!(
                    "{} changed status from {} to {}",
                    bold(&actor),
                    bold(&from),
                    bold(&to)
                ),
            )
        }
        ActivityVerb::Moved => {
            let from = event.from_parent_id.as_ref().and_then(|v| v.as_ref());
            let to = event.to_parent_id.as_ref().and_then(|v| v.as_ref());
            match (from, to) {
                (None, Some(_)) => (
                    format!("{actor} moved this into a container"),
                    format!("{} moved this into a container", bold(&actor)),
                ),
                (Some(_), None) => (
                    format!("{actor} moved this to root level"),
                    format!("{} moved this to root level", bold(&actor)),
                ),
                _ => (
                    format!("{actor} moved this to a new location"),
                    format!("{} moved this to a new location", bold(&actor)),
                ),
            }
        }
        ActivityVerb::Updated => format_updated(&actor, &event.changes),
        ActivityVerb::Commented => {
            let comment = event.meta.get("comment").and_then(JsonValue::as_str);
            if let Some(c) = comment {
                let preview = truncate(c);
                (
                    format!("{actor} commented: \"{preview}\""),
                    format!("{} commented: \"{}\"", bold(&actor), preview),
                )
            } else {
                (
                    format!("{actor} left a comment"),
                    format!("{} left a comment", bold(&actor)),
                )
            }
        }
        ActivityVerb::Mentioned => (
            format!("{actor} mentioned this"),
            format!("{} mentioned this", bold(&actor)),
        ),
        ActivityVerb::Assigned => {
            let assignee = event.meta.get("assigneeName").and_then(JsonValue::as_str);
            if let Some(name) = assignee {
                (
                    format!("{actor} assigned this to {name}"),
                    format!("{} assigned this to {}", bold(&actor), bold(name)),
                )
            } else {
                (
                    format!("{actor} assigned this"),
                    format!("{} assigned this", bold(&actor)),
                )
            }
        }
        ActivityVerb::Unassigned => {
            let assignee = event.meta.get("assigneeName").and_then(JsonValue::as_str);
            if let Some(name) = assignee {
                (
                    format!("{actor} unassigned {name}"),
                    format!("{} unassigned {}", bold(&actor), bold(name)),
                )
            } else {
                (
                    format!("{actor} unassigned this"),
                    format!("{} unassigned this", bold(&actor)),
                )
            }
        }
        ActivityVerb::Attached => {
            let name = event.meta.get("name").and_then(JsonValue::as_str);
            if let Some(name) = name {
                (
                    format!("{actor} attached \"{name}\""),
                    format!("{} attached {}", bold(&actor), bold(name)),
                )
            } else {
                (
                    format!("{actor} added an attachment"),
                    format!("{} added an attachment", bold(&actor)),
                )
            }
        }
        ActivityVerb::Detached => {
            let name = event.meta.get("name").and_then(JsonValue::as_str);
            if let Some(name) = name {
                (
                    format!("{actor} removed attachment \"{name}\""),
                    format!("{} removed attachment {}", bold(&actor), bold(name)),
                )
            } else {
                (
                    format!("{actor} removed an attachment"),
                    format!("{} removed an attachment", bold(&actor)),
                )
            }
        }
        ActivityVerb::Linked => {
            let target = event.meta.get("targetName").and_then(JsonValue::as_str);
            if let Some(t) = target {
                (
                    format!("{actor} linked to \"{t}\""),
                    format!("{} linked to {}", bold(&actor), bold(t)),
                )
            } else {
                (
                    format!("{actor} added a link"),
                    format!("{} added a link", bold(&actor)),
                )
            }
        }
        ActivityVerb::Unlinked => {
            let target = event.meta.get("targetName").and_then(JsonValue::as_str);
            if let Some(t) = target {
                (
                    format!("{actor} removed link to \"{t}\""),
                    format!("{} removed link to {}", bold(&actor), bold(t)),
                )
            } else {
                (
                    format!("{actor} removed a link"),
                    format!("{} removed a link", bold(&actor)),
                )
            }
        }
        ActivityVerb::Completed => (
            format!("{actor} completed this"),
            format!("{} completed this", bold(&actor)),
        ),
        ActivityVerb::Reopened => (
            format!("{actor} reopened this"),
            format!("{} reopened this", bold(&actor)),
        ),
        ActivityVerb::Blocked => {
            let reason = event.meta.get("reason").and_then(JsonValue::as_str);
            if let Some(r) = reason {
                (
                    format!("{actor} blocked this: \"{r}\""),
                    format!("{} blocked this: \"{}\"", bold(&actor), r),
                )
            } else {
                (
                    format!("{actor} blocked this"),
                    format!("{} blocked this", bold(&actor)),
                )
            }
        }
        ActivityVerb::Unblocked => (
            format!("{actor} unblocked this"),
            format!("{} unblocked this", bold(&actor)),
        ),
        ActivityVerb::Custom => {
            let label = event
                .meta
                .get("verb")
                .and_then(JsonValue::as_str)
                .unwrap_or("performed an action");
            (
                format!("{actor} {label}"),
                format!("{} {label}", bold(&actor)),
            )
        }
    };

    ActivityDescription { text, html }
}

fn format_updated(actor: &str, changes: &[FieldChange]) -> (String, String) {
    match changes.len() {
        0 => (
            format!("{actor} updated this"),
            format!("{} updated this", bold(actor)),
        ),
        1 => {
            let c = &changes[0];
            let label = format_field_name(&c.field);
            let from = format_field_value(&c.before);
            let to = format_field_value(&c.after);
            (
                format!("{actor} changed {label} from \"{from}\" to \"{to}\""),
                format!(
                    "{} changed {} from {} to {}",
                    bold(actor),
                    label,
                    bold(&from),
                    bold(&to)
                ),
            )
        }
        _ => {
            let names: Vec<String> = changes
                .iter()
                .take(3)
                .map(|c| format_field_name(&c.field))
                .collect();
            let names_joined = names.join(", ");
            let extra = changes.len().saturating_sub(3);
            let summary = if extra > 0 {
                format!("{names_joined} and {extra} more")
            } else {
                names_joined
            };
            (
                format!("{actor} updated {summary}"),
                format!("{} updated {}", bold(actor), summary),
            )
        }
    }
}

// ── Date grouping ────────────────────────────────────────────────────────────

const ONE_DAY_SEC: i64 = 86_400;

fn start_of_day_utc(ts: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(ts.year(), ts.month(), ts.day(), 0, 0, 0)
        .single()
        .unwrap_or(ts)
}

pub fn group_activity_by_date(events: &[ActivityEvent]) -> Vec<ActivityGroup> {
    group_activity_by_date_at(events, Utc::now())
}

/// Same as [`group_activity_by_date`] but takes `now` as a parameter
/// so tests can stabilise the boundary without mocking the clock.
pub fn group_activity_by_date_at(
    events: &[ActivityEvent],
    now: DateTime<Utc>,
) -> Vec<ActivityGroup> {
    let today = start_of_day_utc(now);
    let yesterday = today - chrono::Duration::seconds(ONE_DAY_SEC);
    let week_ago = today - chrono::Duration::seconds(7 * ONE_DAY_SEC);

    let mut today_bucket: Vec<ActivityEvent> = Vec::new();
    let mut yesterday_bucket: Vec<ActivityEvent> = Vec::new();
    let mut week_bucket: Vec<ActivityEvent> = Vec::new();
    let mut earlier_bucket: Vec<ActivityEvent> = Vec::new();

    for event in events {
        let day = start_of_day_utc(event.created_at);
        if day >= today {
            today_bucket.push(event.clone());
        } else if day >= yesterday {
            yesterday_bucket.push(event.clone());
        } else if day > week_ago {
            week_bucket.push(event.clone());
        } else {
            earlier_bucket.push(event.clone());
        }
    }

    let mut groups = Vec::new();
    for (label, mut bucket) in [
        ("Today", today_bucket),
        ("Yesterday", yesterday_bucket),
        ("This week", week_bucket),
        ("Earlier", earlier_bucket),
    ] {
        if bucket.is_empty() {
            continue;
        }
        bucket.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        groups.push(ActivityGroup {
            label,
            events: bucket,
        });
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::activity::log::{
        ActivityEvent, ActivityVerb, FieldChange,
    };
    use chrono::Duration;
    use serde_json::json;

    fn mk_event(verb: ActivityVerb, created_at: DateTime<Utc>) -> ActivityEvent {
        ActivityEvent {
            id: "test".into(),
            object_id: "obj".into(),
            verb,
            actor_id: None,
            actor_name: None,
            changes: vec![],
            from_parent_id: None,
            to_parent_id: None,
            from_status: None,
            to_status: None,
            meta: Default::default(),
            created_at,
        }
    }

    #[test]
    fn format_field_name_strips_data_prefix() {
        assert_eq!(format_field_name("data.priority"), "priority");
    }

    #[test]
    fn format_field_name_applies_overrides() {
        assert_eq!(format_field_name("parentId"), "parent");
        assert_eq!(format_field_name("endDate"), "end date");
    }

    #[test]
    fn format_field_name_camel_split() {
        assert_eq!(format_field_name("firstName"), "first name");
    }

    #[test]
    fn format_field_name_normalizes_separators() {
        assert_eq!(format_field_name("some_snake_field"), "some snake field");
        assert_eq!(format_field_name("some-kebab-field"), "some kebab field");
    }

    #[test]
    fn format_field_value_handles_primitives() {
        assert_eq!(format_field_value(&json!(null)), "(none)");
        assert_eq!(format_field_value(&json!(true)), "yes");
        assert_eq!(format_field_value(&json!(false)), "no");
        assert_eq!(format_field_value(&json!(42)), "42");
    }

    #[test]
    fn format_field_value_handles_short_strings() {
        assert_eq!(format_field_value(&json!("hello")), "hello");
        assert_eq!(format_field_value(&json!("")), "(empty)");
    }

    #[test]
    fn format_field_value_truncates_long_strings() {
        let long = "x".repeat(80);
        let out = format_field_value(&JsonValue::String(long));
        assert!(out.ends_with('\u{2026}'));
        assert_eq!(out.chars().count(), MAX_TEXT_LENGTH + 1);
    }

    #[test]
    fn format_field_value_array_truncates() {
        let arr = json!([1, 2, 3, 4, 5]);
        assert_eq!(format_field_value(&arr), "1, 2, 3 and 2 more");
        assert_eq!(format_field_value(&json!([])), "(empty)");
    }

    #[test]
    fn format_created_verb() {
        let event = mk_event(ActivityVerb::Created, Utc::now());
        let desc = format_activity(
            &event,
            FormatActivityOptions {
                actor_name: Some("Alice"),
                object_name: Some("Task 1"),
            },
        );
        assert_eq!(desc.text, "Alice created \"Task 1\"");
        assert!(desc.html.contains("<b>Alice</b>"));
    }

    #[test]
    fn format_rename_uses_change_values() {
        let mut event = mk_event(ActivityVerb::Renamed, Utc::now());
        event.changes = vec![FieldChange {
            field: "name".into(),
            before: json!("Old"),
            after: json!("New"),
        }];
        let desc = format_activity(&event, FormatActivityOptions::default());
        assert!(desc.text.contains("Old"));
        assert!(desc.text.contains("New"));
    }

    #[test]
    fn format_status_changed_uses_status_fields() {
        let mut event = mk_event(ActivityVerb::StatusChanged, Utc::now());
        event.from_status = Some("Todo".into());
        event.to_status = Some("Done".into());
        let desc = format_activity(
            &event,
            FormatActivityOptions {
                actor_name: Some("Bob"),
                ..Default::default()
            },
        );
        assert_eq!(
            desc.text,
            "Bob changed status from \"Todo\" to \"Done\""
        );
    }

    #[test]
    fn format_moved_distinguishes_directions() {
        let mut into = mk_event(ActivityVerb::Moved, Utc::now());
        into.from_parent_id = Some(None);
        into.to_parent_id = Some(Some("p".into()));
        assert!(format_activity(&into, FormatActivityOptions::default())
            .text
            .contains("into a container"));

        let mut out = mk_event(ActivityVerb::Moved, Utc::now());
        out.from_parent_id = Some(Some("p".into()));
        out.to_parent_id = Some(None);
        assert!(format_activity(&out, FormatActivityOptions::default())
            .text
            .contains("root level"));

        let mut over = mk_event(ActivityVerb::Moved, Utc::now());
        over.from_parent_id = Some(Some("a".into()));
        over.to_parent_id = Some(Some("b".into()));
        assert!(format_activity(&over, FormatActivityOptions::default())
            .text
            .contains("new location"));
    }

    #[test]
    fn format_updated_single_field() {
        let mut event = mk_event(ActivityVerb::Updated, Utc::now());
        event.changes = vec![FieldChange {
            field: "priority".into(),
            before: json!("low"),
            after: json!("high"),
        }];
        let desc = format_activity(
            &event,
            FormatActivityOptions {
                actor_name: Some("Alice"),
                ..Default::default()
            },
        );
        assert_eq!(
            desc.text,
            "Alice changed priority from \"low\" to \"high\""
        );
    }

    #[test]
    fn format_updated_multi_field_truncates() {
        let mut event = mk_event(ActivityVerb::Updated, Utc::now());
        event.changes = vec![
            FieldChange { field: "a".into(), before: json!(0), after: json!(1) },
            FieldChange { field: "b".into(), before: json!(0), after: json!(1) },
            FieldChange { field: "c".into(), before: json!(0), after: json!(1) },
            FieldChange { field: "d".into(), before: json!(0), after: json!(1) },
        ];
        let desc = format_activity(&event, FormatActivityOptions::default());
        assert_eq!(desc.text, "Someone updated a, b, c and 1 more");
    }

    #[test]
    fn format_custom_uses_meta_verb() {
        let mut event = mk_event(ActivityVerb::Custom, Utc::now());
        event.meta.insert("verb".into(), json!("rebuilt the index"));
        let desc = format_activity(&event, FormatActivityOptions::default());
        assert_eq!(desc.text, "Someone rebuilt the index");
    }

    #[test]
    fn group_activity_partitions_by_bucket() {
        let now = Utc.with_ymd_and_hms(2026, 4, 15, 12, 0, 0).unwrap();
        let today = now;
        let yesterday = now - Duration::hours(24);
        let three_days = now - Duration::days(3);
        let last_month = now - Duration::days(30);

        let events = vec![
            mk_event(ActivityVerb::Updated, today),
            mk_event(ActivityVerb::Updated, yesterday),
            mk_event(ActivityVerb::Updated, three_days),
            mk_event(ActivityVerb::Updated, last_month),
        ];

        let groups = group_activity_by_date_at(&events, now);
        let labels: Vec<&str> = groups.iter().map(|g| g.label).collect();
        assert_eq!(labels, vec!["Today", "Yesterday", "This week", "Earlier"]);
    }

    #[test]
    fn group_activity_skips_empty_buckets() {
        let now = Utc.with_ymd_and_hms(2026, 4, 15, 12, 0, 0).unwrap();
        let events = vec![mk_event(ActivityVerb::Updated, now)];
        let groups = group_activity_by_date_at(&events, now);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].label, "Today");
    }
}
