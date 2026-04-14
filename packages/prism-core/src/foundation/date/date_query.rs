//! Date range + comparison utilities. Port of
//! `foundation/date/date-query.ts`.
//!
//! `YYYY-MM-DD` sorts correctly as a string, so all comparisons are
//! lexical — matching the legacy TS behaviour exactly.

use super::date_format::week_start;
use super::date_math::{add_days, today_iso};

pub fn is_between(date: &str, from: &str, to: &str) -> bool {
    date >= from && date <= to
}

pub fn is_before(date: &str, other: &str) -> bool {
    date < other
}

pub fn is_after(date: &str, other: &str) -> bool {
    date > other
}

pub fn is_today(date: &str) -> bool {
    date == today_iso()
}

pub fn is_past(date: &str) -> bool {
    date < today_iso().as_str()
}

pub fn is_future(date: &str) -> bool {
    date > today_iso().as_str()
}

pub fn min_date<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a <= b {
        a
    } else {
        b
    }
}

pub fn max_date<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a >= b {
        a
    } else {
        b
    }
}

pub fn clamp_date<'a>(date: &'a str, min: &'a str, max: &'a str) -> &'a str {
    if date < min {
        min
    } else if date > max {
        max
    } else {
        date
    }
}

pub fn date_range(from: &str, to: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut cursor = from.to_string();
    while cursor.as_str() <= to {
        results.push(cursor.clone());
        cursor = add_days(&cursor, 1);
    }
    results
}

pub fn weeks_in_range(from: &str, to: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut cursor = week_start(from);
    if cursor.as_str() < from {
        cursor = add_days(&cursor, 7);
    }
    while cursor.as_str() <= to {
        results.push(cursor.clone());
        cursor = add_days(&cursor, 7);
    }
    results
}

pub fn months_in_range(from: &str, to: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut parts_from = from.split('-');
    let mut parts_to = to.split('-');
    let mut year = parts_from
        .next()
        .and_then(|p| p.parse::<i32>().ok())
        .unwrap_or(0);
    let mut month = parts_from
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(1);
    let to_year = parts_to
        .next()
        .and_then(|p| p.parse::<i32>().ok())
        .unwrap_or(0);
    let to_month = parts_to
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(1);

    while year < to_year || (year == to_year && month <= to_month) {
        results.push(format!("{year}-{month:02}-01"));
        month += 1;
        if month > 12 {
            month = 1;
            year += 1;
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_between_is_inclusive() {
        assert!(is_between("2026-03-05", "2026-03-01", "2026-03-31"));
        assert!(is_between("2026-03-01", "2026-03-01", "2026-03-31"));
        assert!(is_between("2026-03-31", "2026-03-01", "2026-03-31"));
        assert!(!is_between("2026-04-01", "2026-03-01", "2026-03-31"));
    }

    #[test]
    fn date_range_is_inclusive() {
        let range = date_range("2026-03-01", "2026-03-03");
        assert_eq!(range, vec!["2026-03-01", "2026-03-02", "2026-03-03"]);
    }

    #[test]
    fn weeks_in_range_emits_mondays() {
        let weeks = weeks_in_range("2026-03-01", "2026-03-31");
        assert!(weeks.iter().all(|w| w.starts_with("2026-03")));
        // Every returned week-start must actually be a Monday.
        for w in &weeks {
            assert_eq!(super::super::day_of_week(w), 1, "{w} must be a Monday");
        }
    }

    #[test]
    fn months_in_range_crosses_year_boundary() {
        let months = months_in_range("2025-11-15", "2026-02-15");
        assert_eq!(
            months,
            vec![
                "2025-11-01".to_string(),
                "2025-12-01".to_string(),
                "2026-01-01".to_string(),
                "2026-02-01".to_string(),
            ]
        );
    }

    #[test]
    fn clamp_date_clamps_both_ways() {
        assert_eq!(
            clamp_date("2026-01-15", "2026-02-01", "2026-03-01"),
            "2026-02-01"
        );
        assert_eq!(
            clamp_date("2026-04-15", "2026-02-01", "2026-03-01"),
            "2026-03-01"
        );
        assert_eq!(
            clamp_date("2026-02-15", "2026-02-01", "2026-03-01"),
            "2026-02-15"
        );
    }
}
