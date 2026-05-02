//! Pure data types for the calendar engine.
//!
//! Port of `@core/calendar` types. Dates use `chrono::NaiveDate`
//! (no timezone — calendar events are date-level).

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

// ── Frequency ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

// ── Weekday ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    /// Convert to `chrono::Weekday`.
    pub fn to_chrono(self) -> chrono::Weekday {
        match self {
            Self::Monday => chrono::Weekday::Mon,
            Self::Tuesday => chrono::Weekday::Tue,
            Self::Wednesday => chrono::Weekday::Wed,
            Self::Thursday => chrono::Weekday::Thu,
            Self::Friday => chrono::Weekday::Fri,
            Self::Saturday => chrono::Weekday::Sat,
            Self::Sunday => chrono::Weekday::Sun,
        }
    }

    /// Convert from `chrono::Weekday`.
    pub fn from_chrono(wd: chrono::Weekday) -> Self {
        match wd {
            chrono::Weekday::Mon => Self::Monday,
            chrono::Weekday::Tue => Self::Tuesday,
            chrono::Weekday::Wed => Self::Wednesday,
            chrono::Weekday::Thu => Self::Thursday,
            chrono::Weekday::Fri => Self::Friday,
            chrono::Weekday::Sat => Self::Saturday,
            chrono::Weekday::Sun => Self::Sunday,
        }
    }

    /// Parse a two-letter RRULE day abbreviation (MO, TU, WE, etc.).
    pub fn from_rrule_str(s: &str) -> Option<Self> {
        match s {
            "MO" => Some(Self::Monday),
            "TU" => Some(Self::Tuesday),
            "WE" => Some(Self::Wednesday),
            "TH" => Some(Self::Thursday),
            "FR" => Some(Self::Friday),
            "SA" => Some(Self::Saturday),
            "SU" => Some(Self::Sunday),
            _ => None,
        }
    }

    /// Two-letter RRULE abbreviation.
    pub fn to_rrule_str(self) -> &'static str {
        match self {
            Self::Monday => "MO",
            Self::Tuesday => "TU",
            Self::Wednesday => "WE",
            Self::Thursday => "TH",
            Self::Friday => "FR",
            Self::Saturday => "SA",
            Self::Sunday => "SU",
        }
    }
}

// ── Recurrence ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurrenceRule {
    pub frequency: Frequency,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<NaiveDate>,
    pub by_day: Vec<Weekday>,
    pub by_month_day: Vec<u32>,
}

impl Default for RecurrenceRule {
    fn default() -> Self {
        Self {
            frequency: Frequency::Daily,
            interval: 1,
            count: None,
            until: None,
            by_day: Vec::new(),
            by_month_day: Vec::new(),
        }
    }
}

// ── Date Range ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

// ── Calendar Event ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub object_id: String,
    pub title: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub all_day: bool,
    pub color: Option<String>,
    pub event_type: String,
}

// ── Event Occurrence ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventOccurrence {
    pub event: CalendarEvent,
    pub instance_date: NaiveDate,
    pub is_first: bool,
    pub is_last: bool,
}

// ── Query Options ─────────────────────────────────────────────────

pub struct CalendarQueryOptions {
    pub range: DateRange,
    pub types: Option<Vec<String>>,
    pub include_recurring: bool,
}

impl CalendarQueryOptions {
    pub fn new(range: DateRange) -> Self {
        Self {
            range,
            types: None,
            include_recurring: true,
        }
    }
}
