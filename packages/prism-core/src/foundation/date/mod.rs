//! `foundation::date` — pure UTC date arithmetic and range queries.
//! Port of `@prism/core/date` (`date-math.ts`, `date-format.ts`,
//! `date-query.ts`).
//!
//! The legacy TS module works on `YYYY-MM-DD` strings; the Rust port
//! preserves that wire shape and the associated lexical comparison
//! semantics used throughout the graph query layer.

pub mod date_format;
pub mod date_math;
pub mod date_query;

pub use date_format::{
    day_of_week, format_display_date, format_short_date, get_day, get_month, get_year, month_end,
    month_start, quarter_start, week_end, week_start, year_start,
};
pub use date_math::{
    add_days, add_months, add_years, days_in_month, diff_days, diff_months, format_date,
    parse_date, today_iso,
};
pub use date_query::{
    clamp_date, date_range, is_after, is_before, is_between, is_future, is_past, is_today,
    max_date, min_date, months_in_range, weeks_in_range,
};
