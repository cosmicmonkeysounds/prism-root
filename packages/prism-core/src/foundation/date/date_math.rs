//! Pure UTC date arithmetic. Port of `foundation/date/date-math.ts`.
//!
//! All dates are `YYYY-MM-DD` strings. The legacy TS module used
//! `Date.UTC` for all math; Rust uses `chrono::NaiveDate` which is
//! already calendar-based and timezone-free, matching the TS
//! behaviour bit-for-bit.

use chrono::{Datelike, Duration, NaiveDate, Utc};

/// Parse a `YYYY-MM-DD` string to a [`NaiveDate`]. Returns
/// `NaiveDate::default()` (year 1, Jan 1) on malformed input,
/// matching the legacy TS default-of-0 behaviour.
pub fn parse_date(iso: &str) -> NaiveDate {
    let mut parts = iso.split('-');
    let year = parts
        .next()
        .and_then(|p| p.parse::<i32>().ok())
        .unwrap_or(0);
    let month = parts
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(1);
    let day = parts
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(1);
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or_default()
}

/// Format a [`NaiveDate`] back to `YYYY-MM-DD`.
pub fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Return today's date (UTC) as a `YYYY-MM-DD` string.
pub fn today_iso() -> String {
    format_date(Utc::now().date_naive())
}

/// Add `n` days (may be negative).
pub fn add_days(iso: &str, n: i64) -> String {
    format_date(parse_date(iso) + Duration::days(n))
}

/// Add `n` months. Month-end safe: Jan 31 + 1 month = Feb 28/29.
pub fn add_months(iso: &str, n: i32) -> String {
    let date = parse_date(iso);
    let base_month = date.month() as i32 - 1 + n;
    let target_year = date.year() + base_month.div_euclid(12);
    let normalized_month = base_month.rem_euclid(12) as u32;
    let max_day = days_in_month(target_year, normalized_month);
    let clamped_day = date.day().min(max_day);
    let result =
        NaiveDate::from_ymd_opt(target_year, normalized_month + 1, clamped_day).unwrap_or_default();
    format_date(result)
}

/// Add `n` years (applied as `n * 12` months for month-end safety).
pub fn add_years(iso: &str, n: i32) -> String {
    add_months(iso, n * 12)
}

/// `to - from` in days. May be negative.
pub fn diff_days(from: &str, to: &str) -> i64 {
    (parse_date(to) - parse_date(from)).num_days()
}

/// Approximate month difference — purely by `(year, month)` count.
pub fn diff_months(from: &str, to: &str) -> i32 {
    let a = parse_date(from);
    let b = parse_date(to);
    (b.year() - a.year()) * 12 + (b.month() as i32 - a.month() as i32)
}

/// Days in the given UTC year + 0-indexed month (matches legacy
/// `daysInMonth(year, month)` where month is 0..=11).
pub fn days_in_month(year: i32, month: u32) -> u32 {
    let (y, m) = if month >= 11 {
        (year + 1, 1)
    } else {
        (year, month + 2)
    };
    // First of the *next* month minus one day = last day of the
    // target month.
    NaiveDate::from_ymd_opt(y, m, 1)
        .map(|d| (d - Duration::days(1)).day())
        .unwrap_or(28)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format_round_trip() {
        assert_eq!(format_date(parse_date("2026-03-15")), "2026-03-15");
    }

    #[test]
    fn add_days_crosses_month_boundary() {
        assert_eq!(add_days("2026-01-31", 1), "2026-02-01");
        assert_eq!(add_days("2026-03-01", -1), "2026-02-28");
    }

    #[test]
    fn add_months_is_month_end_safe() {
        assert_eq!(add_months("2026-01-31", 1), "2026-02-28");
        assert_eq!(add_months("2024-01-31", 1), "2024-02-29"); // leap
        assert_eq!(add_months("2026-03-15", -2), "2026-01-15");
    }

    #[test]
    fn diff_days_and_months() {
        assert_eq!(diff_days("2026-03-01", "2026-03-10"), 9);
        assert_eq!(diff_months("2026-01-01", "2026-05-01"), 4);
        assert_eq!(diff_months("2026-05-01", "2026-01-01"), -4);
    }

    #[test]
    fn days_in_month_handles_leap_years() {
        // Month is 0-indexed to match legacy TS API.
        assert_eq!(days_in_month(2024, 1), 29); // Feb 2024 (leap)
        assert_eq!(days_in_month(2025, 1), 28); // Feb 2025
        assert_eq!(days_in_month(2026, 0), 31); // Jan
    }
}
