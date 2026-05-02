//! Calendar engine — date helpers, recurrence expansion, calendar queries.
//!
//! Port of `@core/calendar` logic. Hand-rolled RRULE subset — no
//! external `rrule` crate.

use chrono::{Datelike, Days, Months, NaiveDate, Utc};
use indexmap::IndexMap;
use serde_json::Value;

use crate::foundation::object_model::types::GraphObject;

use super::types::{
    CalendarEvent, CalendarQueryOptions, DateRange, EventOccurrence, Frequency, RecurrenceRule,
    Weekday,
};

/// Safety cap on total occurrences from a single recurrence expansion.
const MAX_OCCURRENCES: usize = 10_000;

// ── Date string helpers ───────────────────────────────────────────

/// Format a `NaiveDate` as `YYYY-MM-DD`.
pub fn to_date_string(date: &NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Parse a date string. Accepts `YYYY-MM-DD` and also
/// `YYYY-MM-DDTHH:MM:SS...` (the date prefix is extracted).
pub fn parse_date_str(s: &str) -> Option<NaiveDate> {
    // Try full ISO date first.
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    // Try extracting the date portion from a datetime string.
    if s.len() >= 10 {
        let date_part = &s[..10];
        if let Ok(d) = NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
            return Some(d);
        }
    }
    None
}

// ── GraphObject → CalendarEvent ───────────────────────────────────

/// Convert a `GraphObject` to a `CalendarEvent`. Returns `None` if
/// the object has no parseable `date` field.
pub fn to_calendar_event(obj: &GraphObject) -> Option<CalendarEvent> {
    let date_str = obj.date.as_deref()?;
    let start = parse_date_str(date_str)?;

    // End date: try top-level `end_date`, then `data["endDate"]`, fall back to start.
    let end = obj
        .end_date
        .as_deref()
        .and_then(parse_date_str)
        .or_else(|| {
            obj.data
                .get("endDate")
                .and_then(|v| v.as_str())
                .and_then(parse_date_str)
        })
        .unwrap_or(start);

    // All-day detection: if the date string contains 'T', it has a time component.
    let all_day = !date_str.contains('T');

    Some(CalendarEvent {
        object_id: obj.id.0.clone(),
        title: obj.name.clone(),
        start,
        end,
        all_day,
        color: obj.color.clone(),
        event_type: obj.type_name.clone(),
    })
}

/// Convert a slice of `GraphObject`s to calendar events, discarding
/// any that lack a parseable date.
pub fn to_calendar_events(objects: &[GraphObject]) -> Vec<CalendarEvent> {
    objects.iter().filter_map(to_calendar_event).collect()
}

// ── Range checks ──────────────────────────────────────────────────

/// Check whether an event overlaps with a date range (inclusive on
/// both ends).
pub fn event_in_range(event: &CalendarEvent, range: &DateRange) -> bool {
    event.start <= range.to && event.end >= range.from
}

/// Enumerate every date in a range (inclusive on both ends).
pub fn dates_in_range(range: &DateRange) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    let mut d = range.from;
    while d <= range.to {
        dates.push(d);
        d = d.succ_opt().unwrap_or(d);
        if d == range.to && dates.last() == Some(&d) {
            break; // overflow guard
        }
    }
    dates
}

/// Group events by their start date (insertion-ordered).
pub fn group_events_by_date(events: &[CalendarEvent]) -> IndexMap<NaiveDate, Vec<&CalendarEvent>> {
    let mut map: IndexMap<NaiveDate, Vec<&CalendarEvent>> = IndexMap::new();
    for ev in events {
        map.entry(ev.start).or_default().push(ev);
    }
    map
}

// ── Recurrence parsing ────────────────────────────────────────────

/// Parse a recurrence rule from a `GraphObject`'s `data` map.
/// Looks for `data["recurrence"]` (JSON object) or `data["rrule"]`
/// (RRULE string).
pub fn parse_recurrence_rule(obj: &GraphObject) -> Option<RecurrenceRule> {
    // Try structured JSON object first.
    if let Some(rec) = obj.data.get("recurrence") {
        if let Some(rule) = parse_recurrence_value(rec) {
            return Some(rule);
        }
    }
    // Try RRULE string.
    if let Some(rrule) = obj.data.get("rrule") {
        if let Some(s) = rrule.as_str() {
            return parse_rrule_string(s);
        }
    }
    None
}

/// Parse a recurrence rule from a `serde_json::Value` object.
fn parse_recurrence_value(val: &Value) -> Option<RecurrenceRule> {
    let obj = val.as_object()?;

    let freq_str = obj.get("frequency")?.as_str()?;
    let frequency = match freq_str.to_uppercase().as_str() {
        "DAILY" | "daily" => Frequency::Daily,
        "WEEKLY" | "weekly" => Frequency::Weekly,
        "MONTHLY" | "monthly" => Frequency::Monthly,
        "YEARLY" | "yearly" => Frequency::Yearly,
        _ => return None,
    };

    let interval = obj.get("interval").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

    let count = obj.get("count").and_then(|v| v.as_u64()).map(|n| n as u32);

    let until = obj
        .get("until")
        .and_then(|v| v.as_str())
        .and_then(parse_date_str);

    let by_day = obj
        .get("byDay")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().and_then(Weekday::from_rrule_str))
                .collect()
        })
        .unwrap_or_default();

    let by_month_day = obj
        .get("byMonthDay")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default();

    Some(RecurrenceRule {
        frequency,
        interval,
        count,
        until,
        by_day,
        by_month_day,
    })
}

/// Parse an RRULE string (e.g. `FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE,FR`).
pub fn parse_rrule_string(rrule: &str) -> Option<RecurrenceRule> {
    // Strip leading "RRULE:" if present.
    let s = rrule.strip_prefix("RRULE:").unwrap_or(rrule);

    let mut frequency: Option<Frequency> = None;
    let mut interval: u32 = 1;
    let mut count: Option<u32> = None;
    let mut until: Option<NaiveDate> = None;
    let mut by_day: Vec<Weekday> = Vec::new();
    let mut by_month_day: Vec<u32> = Vec::new();

    for part in s.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((key, value)) = part.split_once('=') {
            match key {
                "FREQ" => {
                    frequency = match value {
                        "DAILY" => Some(Frequency::Daily),
                        "WEEKLY" => Some(Frequency::Weekly),
                        "MONTHLY" => Some(Frequency::Monthly),
                        "YEARLY" => Some(Frequency::Yearly),
                        _ => None,
                    };
                }
                "INTERVAL" => {
                    interval = value.parse().unwrap_or(1);
                }
                "COUNT" => {
                    count = value.parse().ok();
                }
                "UNTIL" => {
                    until = parse_date_str(value);
                }
                "BYDAY" => {
                    by_day = value
                        .split(',')
                        .filter_map(|d| Weekday::from_rrule_str(d.trim()))
                        .collect();
                }
                "BYMONTHDAY" => {
                    by_month_day = value
                        .split(',')
                        .filter_map(|d| d.trim().parse().ok())
                        .collect();
                }
                _ => {}
            }
        }
    }

    Some(RecurrenceRule {
        frequency: frequency?,
        interval,
        count,
        until,
        by_day,
        by_month_day,
    })
}

// ── Recurrence expansion ──────────────────────────────────────────

/// Expand a recurring event into individual occurrences within a
/// date range. Respects `count` (total occurrences, not just
/// in-range) and `until`. Safety cap: 10,000 occurrences.
pub fn expand_recurring(
    event: &CalendarEvent,
    rule: &RecurrenceRule,
    range: &DateRange,
) -> Vec<EventOccurrence> {
    let mut results = Vec::new();
    let mut total_count: usize = 0;
    let max_count = rule.count.map(|c| c as usize).unwrap_or(MAX_OCCURRENCES);
    let effective_max = max_count.min(MAX_OCCURRENCES);

    match rule.frequency {
        Frequency::Daily => {
            let mut current = event.start;
            while total_count < effective_max {
                if let Some(u) = rule.until {
                    if current > u {
                        break;
                    }
                }
                if current > range.to {
                    break;
                }
                if current >= range.from {
                    results.push(make_occurrence(
                        event,
                        current,
                        total_count == 0,
                        false, // set later
                    ));
                }
                total_count += 1;
                current = advance_days(current, rule.interval);
            }
        }
        Frequency::Weekly => {
            if rule.by_day.is_empty() {
                // Simple weekly: recur on the same day of the week.
                let mut current = event.start;
                while total_count < effective_max {
                    if let Some(u) = rule.until {
                        if current > u {
                            break;
                        }
                    }
                    if current > range.to {
                        break;
                    }
                    if current >= range.from {
                        results.push(make_occurrence(event, current, total_count == 0, false));
                    }
                    total_count += 1;
                    current = advance_days(current, 7 * rule.interval);
                }
            } else {
                // Weekly with by_day: iterate day-by-day within each
                // interval-week, keeping only days that match by_day.
                let mut week_start = iso_week_start(event.start);
                while total_count < effective_max {
                    if let Some(u) = rule.until {
                        if week_start > u {
                            break;
                        }
                    }
                    // Check if entire week is past range.
                    let week_end = advance_days(week_start, 6);
                    if week_start > range.to {
                        break;
                    }

                    for day_offset in 0..7u32 {
                        if total_count >= effective_max {
                            break;
                        }
                        let day = advance_days(week_start, day_offset);
                        if day < event.start {
                            continue;
                        }
                        if let Some(u) = rule.until {
                            if day > u {
                                break;
                            }
                        }
                        let wd = Weekday::from_chrono(day.weekday());
                        if rule.by_day.contains(&wd) {
                            if day >= range.from && day <= range.to {
                                results.push(make_occurrence(event, day, total_count == 0, false));
                            }
                            total_count += 1;
                        }
                    }
                    // Advance by interval weeks.
                    week_start = advance_days(week_start, 7 * rule.interval);
                    let _ = week_end; // used only for the break check above
                }
            }
        }
        Frequency::Monthly => {
            let mut month_offset: u32 = 0;
            while total_count < effective_max {
                let candidate = add_months(event.start, month_offset);
                if let Some(current) = candidate {
                    if let Some(u) = rule.until {
                        if current > u {
                            break;
                        }
                    }
                    if current > range.to {
                        break;
                    }

                    if rule.by_month_day.is_empty() {
                        // Recur on the same day of the month.
                        if current >= range.from {
                            results.push(make_occurrence(event, current, total_count == 0, false));
                        }
                        total_count += 1;
                    } else {
                        // Only keep dates whose day-of-month is in by_month_day.
                        for &md in &rule.by_month_day {
                            if total_count >= effective_max {
                                break;
                            }
                            if let Some(d) =
                                NaiveDate::from_ymd_opt(current.year(), current.month(), md)
                            {
                                if let Some(u) = rule.until {
                                    if d > u {
                                        continue;
                                    }
                                }
                                if d >= range.from && d <= range.to {
                                    results.push(make_occurrence(
                                        event,
                                        d,
                                        total_count == 0,
                                        false,
                                    ));
                                }
                                total_count += 1;
                            }
                        }
                    }
                } else {
                    // Couldn't compute month — skip it.
                    total_count += 1;
                }
                month_offset += rule.interval;
            }
        }
        Frequency::Yearly => {
            let mut year_offset: u32 = 0;
            while total_count < effective_max {
                let target_year = event.start.year() + year_offset as i32;
                let candidate =
                    NaiveDate::from_ymd_opt(target_year, event.start.month(), event.start.day());
                if let Some(candidate) = candidate {
                    if let Some(u) = rule.until {
                        if candidate > u {
                            break;
                        }
                    }
                    if candidate > range.to {
                        break;
                    }
                    if candidate >= range.from {
                        results.push(make_occurrence(event, candidate, total_count == 0, false));
                    }
                    total_count += 1;
                } else {
                    // e.g. Feb 29 in a non-leap year — skip.
                    total_count += 1;
                }
                year_offset += rule.interval;
            }
        }
    }

    // Mark the last occurrence.
    if let Some(last) = results.last_mut() {
        // If the series is bounded (count or until), mark the last one
        // generated as `is_last`. We mark it only when the series
        // actually terminated (hit count or until), not when it just
        // exceeded the range window.
        let series_terminated = rule.count.is_some() || rule.until.is_some();
        if series_terminated {
            last.is_last = true;
        }
    }

    results
}

/// Expand a single `GraphObject` into calendar events within a date
/// range, handling recurrence if present.
pub fn expand_event(obj: &GraphObject, range: &DateRange) -> Vec<CalendarEvent> {
    let event = match to_calendar_event(obj) {
        Some(e) => e,
        None => return Vec::new(),
    };

    if let Some(rule) = parse_recurrence_rule(obj) {
        expand_recurring(&event, &rule, range)
            .into_iter()
            .map(|occ| {
                let duration = event.end - event.start;
                CalendarEvent {
                    object_id: event.object_id.clone(),
                    title: event.title.clone(),
                    start: occ.instance_date,
                    end: occ.instance_date + duration,
                    all_day: event.all_day,
                    color: event.color.clone(),
                    event_type: event.event_type.clone(),
                }
            })
            .collect()
    } else if event_in_range(&event, range) {
        vec![event]
    } else {
        Vec::new()
    }
}

/// Expand a slice of `GraphObject`s into calendar events within a
/// date range, handling recurrence.
pub fn expand_events(objects: &[GraphObject], range: &DateRange) -> Vec<CalendarEvent> {
    objects
        .iter()
        .flat_map(|obj| expand_event(obj, range))
        .collect()
}

// ── Calendar query ────────────────────────────────────────────────

/// Query calendar events from a slice of `GraphObject`s with
/// filtering by type and optional recurrence expansion.
pub fn query_calendar(objects: &[GraphObject], opts: &CalendarQueryOptions) -> Vec<CalendarEvent> {
    let filtered: Vec<&GraphObject> = objects
        .iter()
        .filter(|obj| {
            if let Some(types) = &opts.types {
                types.contains(&obj.type_name)
            } else {
                true
            }
        })
        .collect();

    if opts.include_recurring {
        filtered
            .iter()
            .flat_map(|obj| expand_event(obj, &opts.range))
            .collect()
    } else {
        let events = to_calendar_events(&filtered.into_iter().cloned().collect::<Vec<_>>());
        events
            .into_iter()
            .filter(|ev| event_in_range(ev, &opts.range))
            .collect()
    }
}

// ── Range constructors ────────────────────────────────────────────

/// ISO Mon–Sun week range containing `date`.
pub fn week_range(date: NaiveDate) -> DateRange {
    let from = iso_week_start(date);
    let to = advance_days(from, 6);
    DateRange { from, to }
}

/// Full calendar month range for the given year/month.
pub fn month_range(year: i32, month: u32) -> DateRange {
    let from = NaiveDate::from_ymd_opt(year, month, 1).expect("invalid month");
    let to = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("invalid date")
    .pred_opt()
    .expect("invalid date");
    DateRange { from, to }
}

/// Single-day range for today (UTC).
pub fn today_range() -> DateRange {
    let today = Utc::now().date_naive();
    DateRange {
        from: today,
        to: today,
    }
}

/// Split a date range into ISO Mon–Sun week sub-ranges. Partial
/// weeks at the boundaries are included.
pub fn split_into_weeks(range: &DateRange) -> Vec<DateRange> {
    let mut weeks = Vec::new();
    let mut week_start = iso_week_start(range.from);

    while week_start <= range.to {
        let week_end = advance_days(week_start, 6);
        weeks.push(DateRange {
            from: week_start.max(range.from),
            to: week_end.min(range.to),
        });
        week_start = advance_days(week_start, 7);
    }

    weeks
}

// ── Helpers ───────────────────────────────────────────────────────

fn make_occurrence(
    event: &CalendarEvent,
    instance_date: NaiveDate,
    is_first: bool,
    is_last: bool,
) -> EventOccurrence {
    EventOccurrence {
        event: event.clone(),
        instance_date,
        is_first,
        is_last,
    }
}

fn advance_days(date: NaiveDate, days: u32) -> NaiveDate {
    date.checked_add_days(Days::new(days as u64))
        .unwrap_or(date)
}

fn add_months(date: NaiveDate, months: u32) -> Option<NaiveDate> {
    date.checked_add_months(Months::new(months))
}

fn iso_week_start(date: NaiveDate) -> NaiveDate {
    let wd = date.weekday().num_days_from_monday();
    date.checked_sub_days(Days::new(wd as u64)).unwrap_or(date)
}

// ── Widget contributions ─────────────────────────────────────────

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        FieldSpec, LayoutDirection, NumericBounds, SelectOption, SignalSpec, TemplateNode,
        ToolbarAction, WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "calendar-month-view".into(),
            label: "Calendar".into(),
            description: "Month grid showing events with day/week/month views".into(),
            icon: Some("calendar".into()),
            category: WidgetCategory::Temporal,
            config_fields: vec![
                FieldSpec::select(
                    "view_mode",
                    "View Mode",
                    vec![
                        SelectOption::new("month", "Month"),
                        SelectOption::new("week", "Week"),
                        SelectOption::new("day", "Day"),
                    ],
                ),
                FieldSpec::boolean("show_weekends", "Show Weekends").with_default(json!(true)),
            ],
            signals: vec![
                SignalSpec::new("event-selected", "An event was selected")
                    .with_payload(vec![FieldSpec::text("event_id", "Event ID")]),
                SignalSpec::new("date-selected", "A date was clicked")
                    .with_payload(vec![FieldSpec::text("date", "Date")]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("today", "Today", "calendar"),
                ToolbarAction::signal("prev", "Previous", "arrow-left"),
                ToolbarAction::signal("next", "Next", "arrow-right"),
            ],
            default_size: WidgetSize::new(3, 2),
            min_size: Some(WidgetSize::new(2, 1)),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::DataBinding {
                            field: "title".into(),
                            component_id: "heading".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::Repeater {
                            source: "events".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "event"}),
                            }),
                            empty_label: Some("No events".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "calendar-agenda".into(),
            label: "Agenda".into(),
            description: "Upcoming events list".into(),
            icon: Some("list".into()),
            category: WidgetCategory::Temporal,
            config_fields: vec![FieldSpec::number(
                "days_ahead",
                "Days Ahead",
                NumericBounds::min_max(1.0, 90.0),
            )
            .with_default(json!(7))],
            signals: vec![SignalSpec::new("event-selected", "An event was selected")
                .with_payload(vec![FieldSpec::text("event_id", "Event ID")])],
            toolbar_actions: vec![ToolbarAction::signal("refresh", "Refresh", "refresh")],
            default_size: WidgetSize::new(2, 2),
            min_size: Some(WidgetSize::new(1, 1)),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Upcoming", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "events".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "event"}),
                            }),
                            empty_label: Some("No upcoming events".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "calendar-mini".into(),
            label: "Mini Calendar".into(),
            description: "Compact date picker".into(),
            icon: Some("calendar".into()),
            category: WidgetCategory::Temporal,
            config_fields: vec![FieldSpec::boolean("show_week_numbers", "Show Week Numbers")],
            signals: vec![SignalSpec::new("date-selected", "A date was selected")
                .with_payload(vec![FieldSpec::text("date", "Date")])],
            default_size: WidgetSize::new(1, 1),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![TemplateNode::DataBinding {
                        field: "current_date".into(),
                        component_id: "text".into(),
                        prop_key: "body".into(),
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
    use crate::foundation::object_model::types::GraphObject;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn make_obj(id: &str, name: &str, date: &str) -> GraphObject {
        let mut obj = GraphObject::new(id, "event", name);
        obj.date = Some(date.to_string());
        obj
    }

    fn make_obj_with_end(id: &str, name: &str, date: &str, end: &str) -> GraphObject {
        let mut obj = make_obj(id, name, date);
        obj.end_date = Some(end.to_string());
        obj
    }

    // ── Date parsing / formatting ─────────────────────────────────

    #[test]
    fn to_date_string_formats_correctly() {
        let date = d("2026-03-15");
        assert_eq!(to_date_string(&date), "2026-03-15");
    }

    #[test]
    fn parse_date_str_plain_date() {
        assert_eq!(parse_date_str("2026-03-15"), Some(d("2026-03-15")));
    }

    #[test]
    fn parse_date_str_datetime() {
        assert_eq!(
            parse_date_str("2026-03-15T10:30:00Z"),
            Some(d("2026-03-15"))
        );
    }

    #[test]
    fn parse_date_str_invalid() {
        assert_eq!(parse_date_str("not-a-date"), None);
        assert_eq!(parse_date_str(""), None);
    }

    // ── to_calendar_event ─────────────────────────────────────────

    #[test]
    fn to_calendar_event_basic() {
        let obj = make_obj("e1", "Meeting", "2026-05-01");
        let ev = to_calendar_event(&obj).unwrap();
        assert_eq!(ev.object_id, "e1");
        assert_eq!(ev.title, "Meeting");
        assert_eq!(ev.start, d("2026-05-01"));
        assert_eq!(ev.end, d("2026-05-01")); // no end date → same as start
        assert!(ev.all_day);
        assert_eq!(ev.event_type, "event");
    }

    #[test]
    fn to_calendar_event_with_end_date() {
        let obj = make_obj_with_end("e2", "Conference", "2026-05-01", "2026-05-03");
        let ev = to_calendar_event(&obj).unwrap();
        assert_eq!(ev.start, d("2026-05-01"));
        assert_eq!(ev.end, d("2026-05-03"));
    }

    #[test]
    fn to_calendar_event_end_from_data() {
        let mut obj = make_obj("e3", "Sprint", "2026-05-01");
        obj.data.insert(
            "endDate".to_string(),
            serde_json::Value::String("2026-05-14".to_string()),
        );
        let ev = to_calendar_event(&obj).unwrap();
        assert_eq!(ev.end, d("2026-05-14"));
    }

    #[test]
    fn to_calendar_event_datetime_not_all_day() {
        let obj = make_obj("e4", "Call", "2026-05-01T14:00:00Z");
        let ev = to_calendar_event(&obj).unwrap();
        assert!(!ev.all_day);
    }

    #[test]
    fn to_calendar_event_no_date_returns_none() {
        let obj = GraphObject::new("e5", "event", "No date");
        assert!(to_calendar_event(&obj).is_none());
    }

    #[test]
    fn to_calendar_event_with_color() {
        let mut obj = make_obj("e6", "Party", "2026-12-31");
        obj.color = Some("#ff0000".to_string());
        let ev = to_calendar_event(&obj).unwrap();
        assert_eq!(ev.color, Some("#ff0000".to_string()));
    }

    #[test]
    fn to_calendar_events_filters_undated() {
        let objs = vec![
            make_obj("a", "Has date", "2026-01-01"),
            GraphObject::new("b", "event", "No date"),
            make_obj("c", "Also dated", "2026-02-01"),
        ];
        let events = to_calendar_events(&objs);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].object_id, "a");
        assert_eq!(events[1].object_id, "c");
    }

    // ── event_in_range ────────────────────────────────────────────

    #[test]
    fn event_in_range_fully_inside() {
        let ev = CalendarEvent {
            object_id: "x".into(),
            title: "T".into(),
            start: d("2026-05-05"),
            end: d("2026-05-06"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        assert!(event_in_range(&ev, &range));
    }

    #[test]
    fn event_in_range_overlaps_start() {
        let ev = CalendarEvent {
            object_id: "x".into(),
            title: "T".into(),
            start: d("2026-04-28"),
            end: d("2026-05-02"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        assert!(event_in_range(&ev, &range));
    }

    #[test]
    fn event_in_range_overlaps_end() {
        let ev = CalendarEvent {
            object_id: "x".into(),
            title: "T".into(),
            start: d("2026-05-30"),
            end: d("2026-06-02"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        assert!(event_in_range(&ev, &range));
    }

    #[test]
    fn event_in_range_outside() {
        let ev = CalendarEvent {
            object_id: "x".into(),
            title: "T".into(),
            start: d("2026-06-01"),
            end: d("2026-06-02"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        assert!(!event_in_range(&ev, &range));
    }

    #[test]
    fn event_in_range_before() {
        let ev = CalendarEvent {
            object_id: "x".into(),
            title: "T".into(),
            start: d("2026-04-01"),
            end: d("2026-04-30"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        assert!(!event_in_range(&ev, &range));
    }

    // ── dates_in_range ────────────────────────────────────────────

    #[test]
    fn dates_in_range_enumerates() {
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-03"),
        };
        let dates = dates_in_range(&range);
        assert_eq!(dates.len(), 3);
        assert_eq!(dates[0], d("2026-05-01"));
        assert_eq!(dates[1], d("2026-05-02"));
        assert_eq!(dates[2], d("2026-05-03"));
    }

    #[test]
    fn dates_in_range_single_day() {
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-01"),
        };
        let dates = dates_in_range(&range);
        assert_eq!(dates.len(), 1);
    }

    // ── group_events_by_date ──────────────────────────────────────

    #[test]
    fn group_events_preserves_order() {
        let events = vec![
            CalendarEvent {
                object_id: "a".into(),
                title: "A".into(),
                start: d("2026-05-02"),
                end: d("2026-05-02"),
                all_day: true,
                color: None,
                event_type: "event".into(),
            },
            CalendarEvent {
                object_id: "b".into(),
                title: "B".into(),
                start: d("2026-05-01"),
                end: d("2026-05-01"),
                all_day: true,
                color: None,
                event_type: "event".into(),
            },
            CalendarEvent {
                object_id: "c".into(),
                title: "C".into(),
                start: d("2026-05-02"),
                end: d("2026-05-02"),
                all_day: true,
                color: None,
                event_type: "event".into(),
            },
        ];
        let grouped = group_events_by_date(&events);
        assert_eq!(grouped.len(), 2);
        // Insertion order: May 2 first, then May 1.
        let keys: Vec<_> = grouped.keys().collect();
        assert_eq!(*keys[0], d("2026-05-02"));
        assert_eq!(*keys[1], d("2026-05-01"));
        assert_eq!(grouped[&d("2026-05-02")].len(), 2);
        assert_eq!(grouped[&d("2026-05-01")].len(), 1);
    }

    // ── Recurrence: daily ─────────────────────────────────────────

    #[test]
    fn expand_daily_basic() {
        let ev = CalendarEvent {
            object_id: "d1".into(),
            title: "Standup".into(),
            start: d("2026-05-01"),
            end: d("2026-05-01"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            interval: 1,
            count: Some(5),
            ..Default::default()
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), 5);
        assert_eq!(occ[0].instance_date, d("2026-05-01"));
        assert_eq!(occ[4].instance_date, d("2026-05-05"));
        assert!(occ[0].is_first);
        assert!(occ[4].is_last);
    }

    #[test]
    fn expand_daily_with_interval() {
        let ev = CalendarEvent {
            object_id: "d2".into(),
            title: "Every other day".into(),
            start: d("2026-05-01"),
            end: d("2026-05-01"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            interval: 2,
            count: Some(3),
            ..Default::default()
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), 3);
        assert_eq!(occ[0].instance_date, d("2026-05-01"));
        assert_eq!(occ[1].instance_date, d("2026-05-03"));
        assert_eq!(occ[2].instance_date, d("2026-05-05"));
    }

    // ── Recurrence: weekly ────────────────────────────────────────

    #[test]
    fn expand_weekly_simple() {
        let ev = CalendarEvent {
            object_id: "w1".into(),
            title: "Weekly".into(),
            start: d("2026-05-04"), // Monday
            end: d("2026-05-04"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Weekly,
            interval: 1,
            count: Some(4),
            ..Default::default()
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), 4);
        assert_eq!(occ[0].instance_date, d("2026-05-04"));
        assert_eq!(occ[1].instance_date, d("2026-05-11"));
        assert_eq!(occ[2].instance_date, d("2026-05-18"));
        assert_eq!(occ[3].instance_date, d("2026-05-25"));
    }

    #[test]
    fn expand_weekly_with_by_day() {
        let ev = CalendarEvent {
            object_id: "w2".into(),
            title: "MWF".into(),
            start: d("2026-05-04"), // Monday
            end: d("2026-05-04"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Weekly,
            interval: 1,
            count: Some(6),
            by_day: vec![Weekday::Monday, Weekday::Wednesday, Weekday::Friday],
            ..Default::default()
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), 6);
        // Week 1: Mon May 4, Wed May 6, Fri May 8
        assert_eq!(occ[0].instance_date, d("2026-05-04"));
        assert_eq!(occ[1].instance_date, d("2026-05-06"));
        assert_eq!(occ[2].instance_date, d("2026-05-08"));
        // Week 2: Mon May 11, Wed May 13, Fri May 15
        assert_eq!(occ[3].instance_date, d("2026-05-11"));
        assert_eq!(occ[4].instance_date, d("2026-05-13"));
        assert_eq!(occ[5].instance_date, d("2026-05-15"));
    }

    #[test]
    fn expand_weekly_by_day_biweekly() {
        let ev = CalendarEvent {
            object_id: "w3".into(),
            title: "Biweekly TuTh".into(),
            start: d("2026-05-04"), // Monday of first week
            end: d("2026-05-04"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Weekly,
            interval: 2,
            count: Some(4),
            by_day: vec![Weekday::Tuesday, Weekday::Thursday],
            ..Default::default()
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-06-30"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), 4);
        // First interval week: Tue May 5, Thu May 7
        assert_eq!(occ[0].instance_date, d("2026-05-05"));
        assert_eq!(occ[1].instance_date, d("2026-05-07"));
        // Next interval week (skip one): Tue May 19, Thu May 21
        assert_eq!(occ[2].instance_date, d("2026-05-19"));
        assert_eq!(occ[3].instance_date, d("2026-05-21"));
    }

    // ── Recurrence: monthly ───────────────────────────────────────

    #[test]
    fn expand_monthly_basic() {
        let ev = CalendarEvent {
            object_id: "m1".into(),
            title: "Monthly".into(),
            start: d("2026-01-15"),
            end: d("2026-01-15"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Monthly,
            interval: 1,
            count: Some(4),
            ..Default::default()
        };
        let range = DateRange {
            from: d("2026-01-01"),
            to: d("2026-12-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), 4);
        assert_eq!(occ[0].instance_date, d("2026-01-15"));
        assert_eq!(occ[1].instance_date, d("2026-02-15"));
        assert_eq!(occ[2].instance_date, d("2026-03-15"));
        assert_eq!(occ[3].instance_date, d("2026-04-15"));
    }

    #[test]
    fn expand_monthly_with_by_month_day() {
        let ev = CalendarEvent {
            object_id: "m2".into(),
            title: "Paydays".into(),
            start: d("2026-01-01"),
            end: d("2026-01-01"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Monthly,
            interval: 1,
            count: Some(6),
            by_month_day: vec![1, 15],
            ..Default::default()
        };
        let range = DateRange {
            from: d("2026-01-01"),
            to: d("2026-12-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), 6);
        assert_eq!(occ[0].instance_date, d("2026-01-01"));
        assert_eq!(occ[1].instance_date, d("2026-01-15"));
        assert_eq!(occ[2].instance_date, d("2026-02-01"));
        assert_eq!(occ[3].instance_date, d("2026-02-15"));
        assert_eq!(occ[4].instance_date, d("2026-03-01"));
        assert_eq!(occ[5].instance_date, d("2026-03-15"));
    }

    // ── Recurrence: yearly ────────────────────────────────────────

    #[test]
    fn expand_yearly_basic() {
        let ev = CalendarEvent {
            object_id: "y1".into(),
            title: "Birthday".into(),
            start: d("2020-06-15"),
            end: d("2020-06-15"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Yearly,
            interval: 1,
            count: Some(3),
            ..Default::default()
        };
        let range = DateRange {
            from: d("2020-01-01"),
            to: d("2025-12-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), 3);
        assert_eq!(occ[0].instance_date, d("2020-06-15"));
        assert_eq!(occ[1].instance_date, d("2021-06-15"));
        assert_eq!(occ[2].instance_date, d("2022-06-15"));
    }

    // ── Until limit ───────────────────────────────────────────────

    #[test]
    fn expand_with_until() {
        let ev = CalendarEvent {
            object_id: "u1".into(),
            title: "Until".into(),
            start: d("2026-05-01"),
            end: d("2026-05-01"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            interval: 1,
            until: Some(d("2026-05-05")),
            ..Default::default()
        };
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), 5); // May 1..5 inclusive
        assert_eq!(occ[4].instance_date, d("2026-05-05"));
    }

    // ── Safety cap ────────────────────────────────────────────────

    #[test]
    fn expand_safety_cap() {
        let ev = CalendarEvent {
            object_id: "cap".into(),
            title: "Infinite".into(),
            start: d("2000-01-01"),
            end: d("2000-01-01"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            interval: 1,
            // No count, no until — unbounded.
            ..Default::default()
        };
        let range = DateRange {
            from: d("2000-01-01"),
            to: d("2100-12-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        assert_eq!(occ.len(), MAX_OCCURRENCES);
    }

    // ── parse_rrule_string ────────────────────────────────────────

    #[test]
    fn parse_rrule_freq_daily() {
        let rule = parse_rrule_string("FREQ=DAILY").unwrap();
        assert_eq!(rule.frequency, Frequency::Daily);
        assert_eq!(rule.interval, 1);
    }

    #[test]
    fn parse_rrule_with_interval() {
        let rule = parse_rrule_string("FREQ=WEEKLY;INTERVAL=2").unwrap();
        assert_eq!(rule.frequency, Frequency::Weekly);
        assert_eq!(rule.interval, 2);
    }

    #[test]
    fn parse_rrule_with_count() {
        let rule = parse_rrule_string("FREQ=MONTHLY;COUNT=12").unwrap();
        assert_eq!(rule.frequency, Frequency::Monthly);
        assert_eq!(rule.count, Some(12));
    }

    #[test]
    fn parse_rrule_with_until() {
        let rule = parse_rrule_string("FREQ=YEARLY;UNTIL=2030-12-31").unwrap();
        assert_eq!(rule.frequency, Frequency::Yearly);
        assert_eq!(rule.until, Some(d("2030-12-31")));
    }

    #[test]
    fn parse_rrule_with_byday() {
        let rule = parse_rrule_string("FREQ=WEEKLY;BYDAY=MO,WE,FR").unwrap();
        assert_eq!(
            rule.by_day,
            vec![Weekday::Monday, Weekday::Wednesday, Weekday::Friday]
        );
    }

    #[test]
    fn parse_rrule_with_bymonthday() {
        let rule = parse_rrule_string("FREQ=MONTHLY;BYMONTHDAY=1,15").unwrap();
        assert_eq!(rule.by_month_day, vec![1, 15]);
    }

    #[test]
    fn parse_rrule_with_prefix() {
        let rule = parse_rrule_string("RRULE:FREQ=DAILY;COUNT=5").unwrap();
        assert_eq!(rule.frequency, Frequency::Daily);
        assert_eq!(rule.count, Some(5));
    }

    #[test]
    fn parse_rrule_invalid_freq() {
        assert!(parse_rrule_string("FREQ=HOURLY").is_none());
    }

    #[test]
    fn parse_rrule_complex() {
        let rule =
            parse_rrule_string("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE,FR;COUNT=10;UNTIL=2026-12-31")
                .unwrap();
        assert_eq!(rule.frequency, Frequency::Weekly);
        assert_eq!(rule.interval, 2);
        assert_eq!(rule.count, Some(10));
        assert_eq!(rule.until, Some(d("2026-12-31")));
        assert_eq!(rule.by_day.len(), 3);
    }

    // ── parse_recurrence_rule from GraphObject ────────────────────

    #[test]
    fn parse_recurrence_from_data_object() {
        let mut obj = make_obj("r1", "Recurring", "2026-05-01");
        obj.data.insert(
            "recurrence".to_string(),
            serde_json::json!({
                "frequency": "weekly",
                "interval": 2,
                "byDay": ["MO", "FR"]
            }),
        );
        let rule = parse_recurrence_rule(&obj).unwrap();
        assert_eq!(rule.frequency, Frequency::Weekly);
        assert_eq!(rule.interval, 2);
        assert_eq!(rule.by_day, vec![Weekday::Monday, Weekday::Friday]);
    }

    #[test]
    fn parse_recurrence_from_rrule_string() {
        let mut obj = make_obj("r2", "Recurring", "2026-05-01");
        obj.data.insert(
            "rrule".to_string(),
            serde_json::Value::String("FREQ=DAILY;COUNT=7".to_string()),
        );
        let rule = parse_recurrence_rule(&obj).unwrap();
        assert_eq!(rule.frequency, Frequency::Daily);
        assert_eq!(rule.count, Some(7));
    }

    #[test]
    fn parse_recurrence_none() {
        let obj = make_obj("r3", "No recurrence", "2026-05-01");
        assert!(parse_recurrence_rule(&obj).is_none());
    }

    // ── query_calendar ────────────────────────────────────────────

    #[test]
    fn query_calendar_with_type_filter() {
        let objs = vec![
            {
                let mut o = make_obj("q1", "Task A", "2026-05-10");
                o.type_name = "task".to_string();
                o
            },
            {
                let mut o = make_obj("q2", "Meeting B", "2026-05-12");
                o.type_name = "meeting".to_string();
                o
            },
            {
                let mut o = make_obj("q3", "Task C", "2026-05-14");
                o.type_name = "task".to_string();
                o
            },
        ];
        let opts = CalendarQueryOptions {
            range: DateRange {
                from: d("2026-05-01"),
                to: d("2026-05-31"),
            },
            types: Some(vec!["task".to_string()]),
            include_recurring: false,
        };
        let events = query_calendar(&objs, &opts);
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.event_type == "task"));
    }

    #[test]
    fn query_calendar_no_type_filter() {
        let objs = vec![
            make_obj("q4", "A", "2026-05-10"),
            make_obj("q5", "B", "2026-05-12"),
        ];
        let opts = CalendarQueryOptions {
            range: DateRange {
                from: d("2026-05-01"),
                to: d("2026-05-31"),
            },
            types: None,
            include_recurring: false,
        };
        let events = query_calendar(&objs, &opts);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn query_calendar_with_recurring() {
        let mut obj = make_obj("q6", "Daily", "2026-05-01");
        obj.data.insert(
            "rrule".to_string(),
            serde_json::Value::String("FREQ=DAILY;COUNT=5".to_string()),
        );
        let objs = vec![obj];
        let opts = CalendarQueryOptions {
            range: DateRange {
                from: d("2026-05-01"),
                to: d("2026-05-31"),
            },
            types: None,
            include_recurring: true,
        };
        let events = query_calendar(&objs, &opts);
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn query_calendar_excludes_out_of_range() {
        let objs = vec![
            make_obj("q7", "In range", "2026-05-15"),
            make_obj("q8", "Out of range", "2026-06-15"),
        ];
        let opts = CalendarQueryOptions {
            range: DateRange {
                from: d("2026-05-01"),
                to: d("2026-05-31"),
            },
            types: None,
            include_recurring: false,
        };
        let events = query_calendar(&objs, &opts);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].object_id, "q7");
    }

    // ── week_range ────────────────────────────────────────────────

    #[test]
    fn week_range_monday() {
        // 2026-05-04 is a Monday
        let range = week_range(d("2026-05-04"));
        assert_eq!(range.from, d("2026-05-04"));
        assert_eq!(range.to, d("2026-05-10"));
    }

    #[test]
    fn week_range_wednesday() {
        // 2026-05-06 is a Wednesday
        let range = week_range(d("2026-05-06"));
        assert_eq!(range.from, d("2026-05-04"));
        assert_eq!(range.to, d("2026-05-10"));
    }

    #[test]
    fn week_range_sunday() {
        // 2026-05-10 is a Sunday
        let range = week_range(d("2026-05-10"));
        assert_eq!(range.from, d("2026-05-04"));
        assert_eq!(range.to, d("2026-05-10"));
    }

    // ── month_range ───────────────────────────────────────────────

    #[test]
    fn month_range_may() {
        let range = month_range(2026, 5);
        assert_eq!(range.from, d("2026-05-01"));
        assert_eq!(range.to, d("2026-05-31"));
    }

    #[test]
    fn month_range_february_non_leap() {
        let range = month_range(2026, 2);
        assert_eq!(range.from, d("2026-02-01"));
        assert_eq!(range.to, d("2026-02-28"));
    }

    #[test]
    fn month_range_february_leap() {
        let range = month_range(2024, 2);
        assert_eq!(range.from, d("2024-02-01"));
        assert_eq!(range.to, d("2024-02-29"));
    }

    #[test]
    fn month_range_december() {
        let range = month_range(2026, 12);
        assert_eq!(range.from, d("2026-12-01"));
        assert_eq!(range.to, d("2026-12-31"));
    }

    // ── split_into_weeks ──────────────────────────────────────────

    #[test]
    fn split_into_weeks_full_month() {
        let range = month_range(2026, 5);
        let weeks = split_into_weeks(&range);
        // May 2026: Fri May 1 .. Sun May 31
        // Week starts: Apr 27, May 4, May 11, May 18, May 25
        // But clamped to range, so first partial, then full weeks.
        assert!(!weeks.is_empty());
        assert_eq!(weeks[0].from, d("2026-05-01"));
        assert_eq!(weeks.last().unwrap().to, d("2026-05-31"));
    }

    #[test]
    fn split_into_weeks_single_day() {
        let range = DateRange {
            from: d("2026-05-06"), // Wednesday
            to: d("2026-05-06"),
        };
        let weeks = split_into_weeks(&range);
        assert_eq!(weeks.len(), 1);
        assert_eq!(weeks[0].from, d("2026-05-06"));
        assert_eq!(weeks[0].to, d("2026-05-06"));
    }

    // ── expand_event / expand_events ──────────────────────────────

    #[test]
    fn expand_event_non_recurring() {
        let obj = make_obj("ne1", "Single", "2026-05-10");
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let events = expand_event(&obj, &range);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Single");
    }

    #[test]
    fn expand_event_recurring() {
        let mut obj = make_obj("re1", "Daily", "2026-05-01");
        obj.data.insert(
            "rrule".to_string(),
            serde_json::Value::String("FREQ=DAILY;COUNT=3".to_string()),
        );
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let events = expand_event(&obj, &range);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn expand_event_out_of_range() {
        let obj = make_obj("oor1", "Out", "2026-06-10");
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let events = expand_event(&obj, &range);
        assert!(events.is_empty());
    }

    #[test]
    fn expand_events_mixed() {
        let mut recurring = make_obj("mix1", "Daily", "2026-05-01");
        recurring.data.insert(
            "rrule".to_string(),
            serde_json::Value::String("FREQ=DAILY;COUNT=3".to_string()),
        );
        let single = make_obj("mix2", "Single", "2026-05-10");
        let out = make_obj("mix3", "Out", "2026-06-01");
        let objs = vec![recurring, single, out];
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let events = expand_events(&objs, &range);
        assert_eq!(events.len(), 4); // 3 recurring + 1 single
    }

    // ── Expand with multi-day events preserves duration ───────────

    #[test]
    fn expand_recurring_preserves_duration() {
        let mut obj = make_obj_with_end("dur1", "Multi-day", "2026-05-01", "2026-05-03");
        obj.data.insert(
            "rrule".to_string(),
            serde_json::Value::String("FREQ=WEEKLY;COUNT=2".to_string()),
        );
        let range = DateRange {
            from: d("2026-05-01"),
            to: d("2026-05-31"),
        };
        let events = expand_event(&obj, &range);
        assert_eq!(events.len(), 2);
        // Duration is 2 days (May 1 to May 3).
        let duration = events[0].end - events[0].start;
        assert_eq!(duration.num_days(), 2);
        let duration2 = events[1].end - events[1].start;
        assert_eq!(duration2.num_days(), 2);
    }

    // ── Count is total, not just in-range ─────────────────────────

    #[test]
    fn count_is_total_not_in_range() {
        let ev = CalendarEvent {
            object_id: "cnt".into(),
            title: "Count test".into(),
            start: d("2026-05-01"),
            end: d("2026-05-01"),
            all_day: true,
            color: None,
            event_type: "event".into(),
        };
        let rule = RecurrenceRule {
            frequency: Frequency::Daily,
            interval: 1,
            count: Some(10),
            ..Default::default()
        };
        // Range starts after some occurrences.
        let range = DateRange {
            from: d("2026-05-06"),
            to: d("2026-05-31"),
        };
        let occ = expand_recurring(&ev, &rule, &range);
        // Total 10 occurrences: May 1..10. In range (May 6..10): 5.
        assert_eq!(occ.len(), 5);
        assert_eq!(occ[0].instance_date, d("2026-05-06"));
        assert_eq!(occ[4].instance_date, d("2026-05-10"));
    }

    // ── Widget contributions ─────────────────────────────────────

    #[test]
    fn calendar_widget_contributions_count_and_ids() {
        let widgets = super::widget_contributions();
        assert_eq!(widgets.len(), 3);
        assert_eq!(widgets[0].id, "calendar-month-view");
        assert_eq!(widgets[1].id, "calendar-agenda");
        assert_eq!(widgets[2].id, "calendar-mini");
    }

    #[test]
    fn calendar_widgets_are_temporal_category() {
        use crate::widget::WidgetCategory;
        let widgets = super::widget_contributions();
        for w in &widgets {
            assert!(matches!(w.category, WidgetCategory::Temporal));
        }
    }

    #[test]
    fn calendar_month_view_has_config_and_signals() {
        let widgets = super::widget_contributions();
        let month = &widgets[0];
        assert_eq!(month.config_fields.len(), 2);
        assert_eq!(month.signals.len(), 2);
        assert_eq!(month.toolbar_actions.len(), 3);
    }
}
