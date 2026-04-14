// @prism/core/date — Date range and comparison utilities.

import { todayISO, addDays } from "./date-math.js";
import { weekStart } from "./date-format.js";

/** Return true if `date` is between `from` and `to` (inclusive). */
export function isBetween(date: string, from: string, to: string): boolean {
  return date >= from && date <= to;
}

/** Return true if `date` is strictly before `other`. */
export function isBefore(date: string, other: string): boolean {
  return date < other;
}

/** Return true if `date` is strictly after `other`. */
export function isAfter(date: string, other: string): boolean {
  return date > other;
}

/** Return true if `date` is today (UTC). */
export function isToday(date: string): boolean {
  return date === todayISO();
}

/** Return true if `date` is in the past (before today UTC). */
export function isPast(date: string): boolean {
  return date < todayISO();
}

/** Return true if `date` is in the future (after today UTC). */
export function isFuture(date: string): boolean {
  return date > todayISO();
}

/** Return the earlier of two ISO dates. */
export function minDate(a: string, b: string): string {
  return a <= b ? a : b;
}

/** Return the later of two ISO dates. */
export function maxDate(a: string, b: string): string {
  return a >= b ? a : b;
}

/** Clamp `date` to the range [min, max]. */
export function clampDate(date: string, min: string, max: string): string {
  if (date < min) return min;
  if (date > max) return max;
  return date;
}

/** Generate an array of ISO date strings for each day in [from, to] inclusive. */
export function dateRange(from: string, to: string): string[] {
  const results: string[] = [];
  let cursor = from;
  while (cursor <= to) {
    results.push(cursor);
    cursor = addDays(cursor, 1);
  }
  return results;
}

/** Generate an array of ISO date strings for each Monday in [from, to]. */
export function weeksInRange(from: string, to: string): string[] {
  const results: string[] = [];
  let cursor = weekStart(from);
  if (cursor < from) {
    cursor = addDays(cursor, 7);
  }
  while (cursor <= to) {
    results.push(cursor);
    cursor = addDays(cursor, 7);
  }
  return results;
}

/** Generate an array of month-start ISO strings for each month in [from, to]. */
export function monthsInRange(from: string, to: string): string[] {
  const results: string[] = [];
  const [fromYear = 0, fromMonth = 1] = from.split("-").map(Number);
  const [toYear = 0, toMonth = 1] = to.split("-").map(Number);

  let year = fromYear;
  let month = fromMonth;

  while (year < toYear || (year === toYear && month <= toMonth)) {
    const monthStr = String(month).padStart(2, "0");
    results.push(`${year}-${monthStr}-01`);
    month++;
    if (month > 12) {
      month = 1;
      year++;
    }
  }

  return results;
}
