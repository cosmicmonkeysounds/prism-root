//! `domain::calendar` — pure-data calendar engine.
//!
//! Port of `@core/calendar` TypeScript module. Date helpers,
//! recurrence expansion (hand-rolled RRULE subset), and calendar
//! queries over `GraphObject`.

pub mod engine;
pub mod types;

pub use engine::{
    dates_in_range, event_in_range, expand_event, expand_events, expand_recurring,
    group_events_by_date, month_range, parse_date_str, parse_recurrence_rule, parse_rrule_string,
    query_calendar, split_into_weeks, to_calendar_event, to_calendar_events, to_date_string,
    today_range, week_range, widget_contributions,
};
pub use types::{
    CalendarEvent, CalendarQueryOptions, DateRange, EventOccurrence, Frequency, RecurrenceRule,
    Weekday,
};
