//! Date component extraction and display utilities. Port of
//! `foundation/date/date-format.ts`.
//!
//! The legacy TS module relies on `Intl.DateTimeFormat` for the
//! localized display helpers; the Rust port hardcodes the
//! equivalent `en-US` short-month output to avoid pulling in a
//! locale crate. When we add `icu` later the locale argument can
//! become meaningful.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

use super::date_math::{add_days, days_in_month, format_date, parse_date};

/// Day of week for an ISO date: `0` = Sunday, `6` = Saturday
/// (matching JS `Date.getUTCDay()`).
pub fn day_of_week(iso: &str) -> u32 {
    match parse_date(iso).weekday() {
        Weekday::Sun => 0,
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
    }
}

/// Monday (ISO week start) for the given date.
pub fn week_start(iso: &str) -> String {
    let dow = day_of_week(iso);
    let offset = if dow == 0 { 6 } else { dow - 1 };
    add_days(iso, -(offset as i64))
}

/// Sunday (ISO week end) for the given date.
pub fn week_end(iso: &str) -> String {
    add_days(&week_start(iso), 6)
}

pub fn month_start(iso: &str) -> String {
    let d = parse_date(iso);
    format_date(NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap_or_default())
}

pub fn month_end(iso: &str) -> String {
    let d = parse_date(iso);
    let last_day = days_in_month(d.year(), d.month() - 1);
    format_date(NaiveDate::from_ymd_opt(d.year(), d.month(), last_day).unwrap_or_default())
}

pub fn quarter_start(iso: &str) -> String {
    let d = parse_date(iso);
    let quarter_first_month = ((d.month() - 1) / 3) * 3 + 1;
    format_date(NaiveDate::from_ymd_opt(d.year(), quarter_first_month, 1).unwrap_or_default())
}

pub fn year_start(iso: &str) -> String {
    let d = parse_date(iso);
    format_date(NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap_or_default())
}

/// Long display form ("Mar 15, 2026"). The `_locale` argument is
/// accepted for API parity with the legacy TS signature but is
/// currently ignored — the output is always en-US style.
pub fn format_display_date(iso: &str, _locale: Option<&str>) -> String {
    let d = parse_date(iso);
    format!("{} {}, {}", short_month(d.month()), d.day(), d.year())
}

pub fn format_short_date(iso: &str, _locale: Option<&str>) -> String {
    let d = parse_date(iso);
    format!("{} {}", short_month(d.month()), d.day())
}

pub fn get_year(iso: &str) -> i32 {
    parse_date(iso).year()
}

pub fn get_month(iso: &str) -> u32 {
    parse_date(iso).month()
}

pub fn get_day(iso: &str) -> u32 {
    parse_date(iso).day()
}

fn short_month(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "",
    }
}

// `Duration` is imported for the doc rendering of `add_days`
// callers; suppress the unused-import warning with a dummy ref.
const _: fn() -> Duration = || Duration::days(0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_start_returns_monday() {
        // 2026-03-15 is a Sunday.
        assert_eq!(week_start("2026-03-15"), "2026-03-09");
    }

    #[test]
    fn month_end_handles_leap_february() {
        assert_eq!(month_end("2024-02-10"), "2024-02-29");
        assert_eq!(month_end("2025-02-10"), "2025-02-28");
    }

    #[test]
    fn quarter_start_snaps_to_quarter() {
        assert_eq!(quarter_start("2026-01-15"), "2026-01-01");
        assert_eq!(quarter_start("2026-05-15"), "2026-04-01");
        assert_eq!(quarter_start("2026-08-15"), "2026-07-01");
        assert_eq!(quarter_start("2026-11-15"), "2026-10-01");
    }

    #[test]
    fn display_formats_match_legacy_en_us() {
        assert_eq!(format_display_date("2026-03-15", None), "Mar 15, 2026");
        assert_eq!(format_short_date("2026-03-15", None), "Mar 15");
    }
}
