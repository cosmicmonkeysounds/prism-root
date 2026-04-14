// @prism/core/date — Date component extraction and display utilities.

import { parseDate, formatDate, addDays, daysInMonth } from "./date-math.js";

/** Day of week for a YYYY-MM-DD string: 0=Sunday … 6=Saturday (UTC). */
export function dayOfWeek(iso: string): number {
  return new Date(parseDate(iso)).getUTCDay();
}

/** Return the Monday (start of ISO week) for the given YYYY-MM-DD. */
export function weekStart(iso: string): string {
  const dow = dayOfWeek(iso);
  const offset = dow === 0 ? 6 : dow - 1;
  return addDays(iso, -offset);
}

/** Return the Sunday (end of ISO week) for the given YYYY-MM-DD. */
export function weekEnd(iso: string): string {
  const start = weekStart(iso);
  return addDays(start, 6);
}

/** Return the first day of the month for the given YYYY-MM-DD. */
export function monthStart(iso: string): string {
  const [year = 0, month = 1] = iso.split("-").map(Number);
  return formatDate(Date.UTC(year, month - 1, 1));
}

/** Return the last day of the month for the given YYYY-MM-DD. */
export function monthEnd(iso: string): string {
  const [year = 0, month = 1] = iso.split("-").map(Number);
  const lastDay = daysInMonth(year, month - 1);
  return formatDate(Date.UTC(year, month - 1, lastDay));
}

/** Return the first day of the quarter for the given YYYY-MM-DD. */
export function quarterStart(iso: string): string {
  const [year = 0, month = 1] = iso.split("-").map(Number);
  const quarterFirstMonth = Math.floor((month - 1) / 3) * 3 + 1;
  return formatDate(Date.UTC(year, quarterFirstMonth - 1, 1));
}

/** Return the first day of the year for the given YYYY-MM-DD. */
export function yearStart(iso: string): string {
  const year = Number(iso.split("-")[0]);
  return formatDate(Date.UTC(year, 0, 1));
}

/**
 * Format a YYYY-MM-DD string for display (e.g. "Mar 15, 2026").
 * Uses Intl.DateTimeFormat — locale-aware but UTC-pinned.
 */
export function formatDisplayDate(iso: string, locale?: string): string {
  const ms = parseDate(iso);
  return new Intl.DateTimeFormat(locale ?? "en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  }).format(ms);
}

/** Format a YYYY-MM-DD string as a short display (e.g. "Mar 15"). */
export function formatShortDate(iso: string, locale?: string): string {
  const ms = parseDate(iso);
  return new Intl.DateTimeFormat(locale ?? "en-US", {
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  }).format(ms);
}

/** Return the ISO year (YYYY) as a number. */
export function getYear(iso: string): number {
  return Number(iso.split("-")[0]);
}

/** Return the ISO month (1–12). */
export function getMonth(iso: string): number {
  return Number(iso.split("-")[1]);
}

/** Return the ISO day of month (1–31). */
export function getDay(iso: string): number {
  return Number(iso.split("-")[2]);
}
