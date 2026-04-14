// @prism/core/date — public barrel
//
// Pure date math utilities — no external dependencies, all UTC-safe.
// All ISO dates are YYYY-MM-DD strings unless noted.
//
// - date-math   — addDays, addMonths, addYears, diffDays, diffMonths, daysInMonth,
//                 parseDate, formatDate, todayISO
// - date-format — dayOfWeek, weekStart, weekEnd, monthStart, monthEnd, quarterStart,
//                 yearStart, formatDisplayDate, formatShortDate, getYear/Month/Day
// - date-query  — isBefore, isAfter, isBetween, isToday, isPast, isFuture, minDate,
//                 maxDate, clampDate, dateRange, weeksInRange, monthsInRange

export * from "./date-math.js";
export * from "./date-format.js";
export * from "./date-query.js";
