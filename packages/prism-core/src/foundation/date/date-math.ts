// @prism/core/date — Pure date arithmetic, all UTC-safe.
// No external dependencies. All ISO dates are YYYY-MM-DD strings.

/** Parse a YYYY-MM-DD string to a UTC midnight timestamp (ms). */
export function parseDate(iso: string): number {
  const [year = 0, month = 1, day = 1] = iso.split("-").map(Number);
  return Date.UTC(year, month - 1, day);
}

/** Format a UTC timestamp (ms) back to YYYY-MM-DD. */
export function formatDate(ms: number): string {
  const d = new Date(ms);
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, "0");
  const dy = String(d.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${dy}`;
}

/** Return today's date as YYYY-MM-DD (UTC). */
export function todayISO(): string {
  return formatDate(Date.now());
}

/** Return the ISO date string for N days from the given ISO date. */
export function addDays(iso: string, n: number): string {
  return formatDate(parseDate(iso) + n * 86_400_000);
}

/**
 * Return the ISO date string for N months from the given ISO date.
 * Month-end safe: Jan 31 + 1 month = Feb 28/29 (not Feb 31).
 */
export function addMonths(iso: string, n: number): string {
  const [year = 0, month = 1, day = 1] = iso.split("-").map(Number);
  const targetMonth = month - 1 + n;
  const targetYear = year + Math.floor(targetMonth / 12);
  const normalizedMonth = ((targetMonth % 12) + 12) % 12;
  const maxDay = daysInMonth(targetYear, normalizedMonth);
  const clampedDay = Math.min(day, maxDay);
  return formatDate(Date.UTC(targetYear, normalizedMonth, clampedDay));
}

/** Return the ISO date string for N years from the given ISO date. */
export function addYears(iso: string, n: number): string {
  return addMonths(iso, n * 12);
}

/** Difference in calendar days between two ISO dates (to - from). May be negative. */
export function diffDays(from: string, to: string): number {
  return Math.round((parseDate(to) - parseDate(from)) / 86_400_000);
}

/** Difference in calendar months (approximate, by month count). */
export function diffMonths(from: string, to: string): number {
  const [fy = 0, fm = 0] = from.split("-").map(Number);
  const [ty = 0, tm = 0] = to.split("-").map(Number);
  return (ty - fy) * 12 + (tm - fm);
}

/** Number of days in the given UTC year+month (0-indexed month). */
export function daysInMonth(year: number, month: number): number {
  return new Date(Date.UTC(year, month + 1, 0)).getUTCDate();
}
